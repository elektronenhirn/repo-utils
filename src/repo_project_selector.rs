use anyhow::{anyhow, bail, Result};
use serde::Deserialize;
use serde_xml_rs::from_reader;
use std::env;
use std::fs;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::path::Path;
use std::path::PathBuf;

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
        selected_projects = selected_projects
            .drain(..)
            .filter(|path| {
                let project = manifest.find_project(path);
                project.is_some() && project.unwrap().in_any_given_group(&groups)
            })
            .collect();
    }

    if let Some(manifest_files) = filter_by_manifest_files {
        let repo_manifests_folder = find_repo_manifests_folder()?;
        let mut aggregated_manifest = Manifest::empty();
        for manifest_file in manifest_files {
            let manifest = parse_manifest(&repo_manifests_folder.join(&manifest_file))?;
            aggregated_manifest.append(&manifest);
        }
        selected_projects = selected_projects
            .drain(..)
            .filter(|p| aggregated_manifest.contains_project(p))
            .collect();
    }

    if include_manifest_repo {
        selected_projects.push(".repo/manifests".to_string());
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
    let find_project_list = find_repo_folder()?.join("project.list");
    match find_project_list.is_file() {
        true => Ok(find_project_list),
        false => Err(anyhow!("no project.list in .repo found")),
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
        for entry in fs::read_dir(&parent)? {
            let entry = entry?;
            if entry.path().is_dir() && entry.file_name() == ".repo" {
                return Ok(parent.to_path_buf());
            }
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
        let path = find_repo_manifests_folder()?.join(include);
        let child = parse(&path).map_err(|e| anyhow!("Failed to parse {}: {}", include, e))?;
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
        let path = path.with_file_name(include);
        let child = parse(&path).map_err(|e| anyhow!("Failed to parse {}: {}", include, e))?;
        manifest.append(&child);
    }
    Ok(manifest)
}

/// OO representation of a repo-tool's manifest xml element
#[derive(Debug, Deserialize)]
pub struct Manifest {
    #[serde(rename = "project", default)]
    pub projects: Vec<Project>,
    #[serde(rename = "include", default)]
    pub includes: Vec<Include>,
}

impl Manifest {
    pub fn empty() -> Self {
        Manifest {
            projects: vec![],
            includes: vec![],
        }
    }

    pub fn append(&mut self, manifest: &Manifest) {
        let projects = &manifest.projects;
        self.projects.extend(projects.iter().cloned());
    }

    pub fn contains_project(&self, local_path: &str) -> bool {
        self.projects.iter().any(|p| p.path == local_path)
    }

    pub fn find_project(&self, local_path: &str) -> Option<&Project> {
        self.projects.iter().find(|p| p.path == local_path)
    }
}

/// OO representation of a repo-tool's project xml element
#[derive(Debug, Deserialize, Clone)]
pub struct Project {
    pub name: String,
    pub path: String,
    pub groups: Option<String>,
}

impl Project {
    pub fn in_any_given_group(&self, test_for_groups: &[String]) -> bool {
        let project_groups: Vec<String> = self
            .groups
            .as_ref()
            .unwrap_or(&String::new())
            .split(&[',', ' '][..])
            .map(|s| s.to_string())
            .collect();
        project_groups
            .iter()
            .any(|g| test_for_groups.iter().any(|other| g == other))
    }
}

/// OO representation of a repo-tool's include xml element
#[derive(Debug, Deserialize, Clone)]
pub struct Include {
    pub name: String,
}
