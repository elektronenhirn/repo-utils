use anyhow::{Result, anyhow, bail};
use quick_xml::de::from_reader;
use serde::Deserialize;
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The repo-tool keeps a list of synced projects at
/// .repo/project.list
/// This function can filter the list of projects by groups
/// and/or manifest files. If a group *and* manifest filter
/// are given, the list will contain the intersection.
/// Additionally the function can include the manifest repo
/// itsself into the list (.repo/manifests).
pub fn select_projects(
    include_manifest_repo: bool,
    filter_by_groups: Option<Vec<String>>,
    filter_by_manifest_files: Option<Vec<PathBuf>>,
) -> Result<Vec<String>> {
    let projects_on_disk = lines_from_file(find_project_list()?)?;
    let mut selected_projects = projects_on_disk;

    if let Some(groups) = filter_by_groups {
        let manifest = parse_manifest(&find_repo_folder()?.join("manifest.xml"))?;
        selected_projects.retain(|path| {
            manifest
                .find_project(path)
                .is_some_and(|project| project.in_any_given_group(&groups))
        });
    }

    if let Some(manifest_files) = filter_by_manifest_files {
        let repo_manifests_folder = find_repo_manifests_folder()?;
        let mut aggregated_manifest = Manifest::default();
        for manifest_file in &manifest_files {
            let manifest = parse_manifest(&repo_manifests_folder.join(manifest_file))?;
            aggregated_manifest.append(&manifest);
        }
        selected_projects.retain(|p| aggregated_manifest.contains_project(p));
    }

    if include_manifest_repo {
        selected_projects.push(".repo/manifests".to_owned());
    }

    Ok(selected_projects)
}

fn lines_from_file(filename: impl AsRef<Path>) -> Result<Vec<String>> {
    BufReader::new(File::open(filename)?)
        .lines()
        .collect::<Result<Vec<_>, _>>()
        .map_err(anyhow::Error::msg)
}

/// returns a path pointing to he project.list file in
/// the .repo folder, or an io::Error in case the file
/// couldn't been found.
pub fn find_project_list() -> Result<PathBuf> {
    let project_list_path = find_repo_folder()?.join("project.list");
    if project_list_path.is_file() {
        Ok(project_list_path)
    } else {
        Err(anyhow!("no project.list in .repo found"))
    }
}

/// returns a path pointing to the .repo folder,
/// or Error in case the .repo folder couldn't been
/// found in the cwd or any of its parent folders.
pub fn find_repo_folder() -> Result<PathBuf> {
    let base_folder = find_repo_root_folder()?;
    Ok(base_folder.join(".repo"))
}

/// returns a path pointing to the .repo/manifests folder,
/// or Error in case the .repo folder couldn't been
/// found in the cwd or any of its parent folders.
pub fn find_repo_manifests_folder() -> Result<PathBuf> {
    let base_folder = find_repo_folder()?;
    Ok(base_folder.join("manifests"))
}

/// returns a path pointing to the folder containing .repo,
/// or io::Error in case the .repo folder couldn't been
/// found in the cwd or any of its parent folders.
pub fn find_repo_root_folder() -> Result<PathBuf> {
    let cwd = env::current_dir()?;
    for parent in cwd.ancestors() {
        let repo_dir = parent.join(".repo");
        if repo_dir.is_dir() {
            return Ok(parent.to_path_buf());
        }
    }
    bail!("no .repo folder found")
}

pub fn parse_manifest(path: &Path) -> Result<Manifest> {
    let file = File::open(path).map_err(|e| anyhow!("Unable to open {:?}: {}", path, e))?;
    let reader = BufReader::new(file);
    let mut manifest: Manifest = from_reader(reader)?;
    let includes: Vec<String> = manifest.includes.iter().map(|i| i.name.clone()).collect();
    for include in &includes {
        let include_path = find_repo_manifests_folder()?.join(include);
        let child =
            parse(&include_path).map_err(|e| anyhow!("Failed to parse {}: {}", include, e))?;
        manifest.append(&child);
    }
    Ok(manifest)
}

pub fn parse(path: &Path) -> Result<Manifest> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut manifest: Manifest = from_reader(reader)?;
    let includes: Vec<String> = manifest.includes.iter().map(|i| i.name.clone()).collect();
    for include in &includes {
        let include_path = path.with_file_name(include);
        let child =
            parse(&include_path).map_err(|e| anyhow!("Failed to parse {}: {}", include, e))?;
        manifest.append(&child);
    }
    Ok(manifest)
}

/// OO representation of a repo-tool's manifest xml element
#[derive(Debug, Deserialize, Default)]
pub struct Manifest {
    #[serde(rename = "project", default)]
    pub projects: Vec<Project>,
    #[serde(rename = "include", default)]
    pub includes: Vec<Include>,
    #[serde(rename = "remote", default)]
    pub remotes: Vec<Remote>,
    pub default: Option<DefaultSettings>,
}

impl Manifest {
    pub fn append(&mut self, manifest: &Manifest) {
        self.projects.extend(manifest.projects.iter().cloned());
        self.remotes.extend(manifest.remotes.iter().cloned());
        if self.default.is_none() {
            self.default.clone_from(&manifest.default);
        }
    }

    pub fn contains_project(&self, local_path: &str) -> bool {
        self.projects.iter().any(|p| p.path == local_path)
    }

    pub fn find_project(&self, local_path: &str) -> Option<&Project> {
        self.projects.iter().find(|p| p.path == local_path)
    }

    /// Returns the remote the given project is fetched from: the
    /// project's own remote or the manifest's default remote.
    fn project_remote(&self, project: &Project) -> Result<&Remote> {
        let remote_name = project
            .remote
            .as_deref()
            .or_else(|| self.default.as_ref().and_then(|d| d.remote.as_deref()))
            .ok_or_else(|| anyhow!("no remote defined for project {}", project.name))?;
        self.remotes
            .iter()
            .find(|r| r.name == remote_name)
            .ok_or_else(|| anyhow!("unknown remote {remote_name} for project {}", project.name))
    }

    /// Returns the URL the given project can be fetched from, resolving
    /// the project's remote (or the default remote) relative to the
    /// URL the manifest has been cloned from.
    pub fn project_url(&self, project: &Project, manifest_url: &str) -> Result<String> {
        let remote = self.project_remote(project)?;
        Ok(join_url(
            &resolve_url(manifest_url, &remote.fetch),
            &project.name,
        ))
    }

    /// Returns the revision (branch, tag or sha) of the given project.
    /// Like in repo, the project's own revision wins over the revision
    /// of its remote, which wins over the manifest's default revision.
    pub fn project_revision<'a>(&'a self, project: &'a Project) -> Option<&'a str> {
        project
            .revision
            .as_deref()
            .or_else(|| {
                self.project_remote(project)
                    .ok()
                    .and_then(|r| r.revision.as_deref())
            })
            .or_else(|| self.default.as_ref().and_then(|d| d.revision.as_deref()))
    }
}

/// Resolves a remote's `fetch` attribute against the URL of the manifest
/// repository, the way the repo tool does (RFC 3986 relative resolution,
/// so `..` on `https://host/platform/manifest` yields `https://host`).
pub fn resolve_url(manifest_url: &str, fetch: &str) -> String {
    let fetch_is_absolute = fetch.contains("://")
        || fetch
            .split_once(':')
            .is_some_and(|(host, _)| !host.is_empty() && !host.contains('/'));
    if fetch_is_absolute {
        return fetch.to_owned();
    }

    // Split the manifest url into a prefix (scheme and host, or the
    // host of an scp-like url) and the path relative segments apply to.
    let (prefix, path) = if let Some(scheme_end) = manifest_url.find("://") {
        let host_start = scheme_end + 3;
        match manifest_url[host_start..].find('/') {
            Some(slash) => manifest_url.split_at(host_start + slash),
            None => (manifest_url, ""),
        }
    } else if let Some(colon) = manifest_url.find(':') {
        manifest_url.split_at(colon + 1)
    } else {
        ("", manifest_url)
    };

    let mut segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    segments.pop(); // the manifest repository itself
    for segment in fetch.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }

    let joined = segments.join("/");
    if prefix.ends_with(':') {
        format!("{prefix}{joined}")
    } else if prefix.is_empty() && !path.starts_with('/') {
        joined
    } else {
        format!("{prefix}/{joined}")
    }
}

/// Appends a project name to a remote's base url
pub fn join_url(base: &str, name: &str) -> String {
    let name = name.trim_start_matches('/');
    if base.is_empty() || base.ends_with(':') || base.ends_with('/') {
        format!("{base}{name}")
    } else {
        format!("{base}/{name}")
    }
}

/// The repo tool maintains a branch tracking the last synced state
/// It is typically named "m/<manifest-branch>" where manifest-branch
/// is the branch used for "repo init".
pub fn lookup_sync_branch_name() -> Result<String> {
    let manifests_folder = find_repo_manifests_folder()?;

    let output = Command::new("sh")
        .current_dir(&manifests_folder)
        .arg("-c")
        .arg("git for-each-ref --format '%(upstream:lstrip=-1)' \"$(git symbolic-ref -q HEAD)\"")
        .output()?;

    if output.status.success() {
        let branch_name = String::from_utf8_lossy(&output.stdout);
        Ok(format!("m/{}", branch_name.trim()))
    } else {
        bail!(String::from_utf8_lossy(&output.stderr).into_owned())
    }
}

/// OO representation of a repo-tool's project xml element
#[derive(Debug, Deserialize, Clone)]
#[serde(from = "RawProject")]
pub struct Project {
    pub name: String,
    /// local path of the project; defaults to the name, like in repo
    pub path: String,
    pub groups: Option<String>,
    pub remote: Option<String>,
    pub revision: Option<String>,
}

#[derive(Deserialize)]
struct RawProject {
    #[serde(rename = "@name")]
    name: String,
    #[serde(rename = "@path")]
    path: Option<String>,
    #[serde(rename = "@groups")]
    groups: Option<String>,
    #[serde(rename = "@remote")]
    remote: Option<String>,
    #[serde(rename = "@revision")]
    revision: Option<String>,
}

impl From<RawProject> for Project {
    fn from(raw: RawProject) -> Self {
        Self {
            path: raw.path.unwrap_or_else(|| raw.name.clone()),
            name: raw.name,
            groups: raw.groups,
            remote: raw.remote,
            revision: raw.revision,
        }
    }
}

impl Project {
    pub fn in_any_given_group(&self, test_for_groups: &[String]) -> bool {
        self.groups
            .as_ref()
            .map(|groups| {
                groups
                    .split([',', ' '])
                    .any(|group| test_for_groups.iter().any(|test_group| group == test_group))
            })
            .unwrap_or(false)
    }
}

/// OO representation of a repo-tool's include xml element
#[derive(Debug, Deserialize, Clone)]
pub struct Include {
    #[serde(rename = "@name")]
    pub name: String,
}

/// OO representation of a repo-tool's remote xml element
#[derive(Debug, Deserialize, Clone)]
pub struct Remote {
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@fetch")]
    pub fetch: String,
    #[serde(rename = "@revision")]
    pub revision: Option<String>,
}

/// OO representation of a repo-tool's default xml element
#[derive(Debug, Deserialize, Clone)]
pub struct DefaultSettings {
    #[serde(rename = "@remote")]
    pub remote: Option<String>,
    #[serde(rename = "@revision")]
    pub revision: Option<String>,
}
