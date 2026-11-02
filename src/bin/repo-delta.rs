use anyhow::{Context, Result, anyhow, bail};
use chrono::NaiveDate;
use clap::Parser;
use colored::*;
use git2::Repository;
use indicatif::{ParallelProgressIterator, ProgressBar, ProgressStyle};
use rayon::prelude::*;
use repo_utils::gerrit_delta::{
    CommitInfo, collect_commits, delete_ref, fetch_revision, find_missing,
};
use repo_utils::repo_project_selector::{
    Manifest, find_repo_root_folder, lookup_sync_branch_name, parse, select_projects,
};
use std::{env, path::Path, process::Command, time::Instant};

const TEMP_REF: &str = "refs/repo-delta/new";

/// Find commits of a repo checkout which got lost when moving to another
/// Gerrit server, by looking up their Change-Ids on the new server,
/// see https://github.com/elektronenhirn/repo-utils
#[derive(Parser, Debug)]
#[command(author, version, long_about = None)]
struct Args {
    /// change working directory (mostly useful for testing)
    #[arg(short = 'C', long, value_name = "DIR", value_hint = clap::ValueHint::DirPath)]
    cwd: Option<std::path::PathBuf>,

    /// ignore projects which are not defined in the given manifest file(s)
    #[arg(short, long, value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
    manifest: Option<Vec<std::path::PathBuf>>,

    /// ignore projects which are not part of the given group(s)
    #[arg(short, long)]
    group: Option<Vec<String>>,

    /// URL of the manifest repository on the new server
    #[arg(long, value_name = "URL")]
    new_manifest_url: String,

    /// branch of the manifest repository on the new server
    /// (default: the branch this checkout was initialized with)
    #[arg(long, value_name = "BRANCH")]
    new_manifest_branch: Option<String>,

    /// manifest file within the new manifest repository
    #[arg(long, value_name = "FILE", default_value = "default.xml")]
    new_manifest_file: String,

    /// only consider commits since the given date (YYYY-MM-DD)
    #[arg(long, value_name = "DATE")]
    since: Option<NaiveDate>,

    /// number of projects to compare in parallel
    #[arg(short, long, default_value = "8")]
    jobs: usize,

    /// keep the temporary refs/repo-delta/new ref in each repository
    #[arg(long, default_value = "false")]
    keep_refs: bool,

    /// Verbose output, e.g. list every missing commit
    #[arg(short, long, default_value = "false")]
    verbose: bool,
}

enum Outcome {
    Compared {
        total: usize,
        missing: Vec<CommitInfo>,
    },
    NotInNewManifest,
    Failed(String),
}

fn main() -> Result<()> {
    let args = Args::parse();

    if let Some(cwd) = &args.cwd {
        env::set_current_dir(cwd)?;
    }

    let projects = select_projects(false, args.group.clone(), args.manifest.clone())?;
    println!("Selected {} projects", projects.len());

    let branch = match &args.new_manifest_branch {
        Some(branch) => branch.clone(),
        None => lookup_sync_branch_name()?
            .strip_prefix("m/")
            .map(str::to_owned)
            .ok_or_else(|| anyhow!("cannot derive manifest branch, use --new-manifest-branch"))?,
    };

    let manifest_checkout = tempfile::tempdir()?;
    let new_manifest = clone_manifest(&args, &branch, manifest_checkout.path())?;

    let since = args
        .since
        .map(|d| d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp());

    let started = Instant::now();
    let repo_root = find_repo_root_folder()?;
    let progress_bar = ProgressBar::new(projects.len() as u64).with_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7}")?,
    );

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(args.jobs.max(1))
        .build()?;
    let mut results: Vec<(String, Outcome)> = pool.install(|| {
        projects
            .par_iter()
            .progress_with(progress_bar)
            .map(|path| {
                let outcome = compare_project(&args, &new_manifest, &repo_root, path, since)
                    .unwrap_or_else(|e| Outcome::Failed(format!("{e:#}")));
                (path.clone(), outcome)
            })
            .collect()
    });
    results.sort_by(|a, b| a.0.cmp(&b.0));

    let mut projects_with_findings = 0;
    let mut missing_commits = 0;
    let mut missing_without_change_id = 0;
    for (path, outcome) in &results {
        match outcome {
            Outcome::Compared { total, missing } => {
                if missing.is_empty() {
                    if args.verbose {
                        println!("{}: all {} commits found", path.green(), total);
                    }
                    continue;
                }
                projects_with_findings += 1;
                missing_commits += missing.len();
                missing_without_change_id +=
                    missing.iter().filter(|c| c.change_id.is_none()).count();
                println!(
                    "{}: {} of {} commits missing",
                    path.red(),
                    missing.len(),
                    total
                );
                if args.verbose {
                    for commit in missing {
                        println!(
                            "    {:.10} {} {}",
                            commit.oid,
                            commit.change_id.as_deref().unwrap_or("<no Change-Id>"),
                            commit.summary
                        );
                    }
                }
            }
            Outcome::NotInNewManifest => {
                projects_with_findings += 1;
                println!("{}: project missing in new manifest", path.red());
            }
            Outcome::Failed(reason) => {
                projects_with_findings += 1;
                println!("{}: comparison failed: {}", path.red(), reason);
            }
        }
    }

    println!();
    println!(
        "Finished in {}s: {}/{} projects with findings, {} commits missing ({} of them without Change-Id)",
        started.elapsed().as_secs(),
        projects_with_findings,
        projects.len(),
        missing_commits,
        missing_without_change_id,
    );

    if projects_with_findings > 0 {
        std::process::exit(1);
    }
    Ok(())
}

fn clone_manifest(args: &Args, branch: &str, into: &Path) -> Result<Manifest> {
    let output = Command::new("git")
        .args(["clone", "--quiet", "--depth", "1", "--branch", branch])
        .arg(&args.new_manifest_url)
        .arg(into)
        .output()
        .context("failed to execute git")?;
    if !output.status.success() {
        bail!(
            "cloning {} failed: {}",
            args.new_manifest_url,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    parse(&into.join(&args.new_manifest_file))
}

fn compare_project(
    args: &Args,
    new_manifest: &Manifest,
    repo_root: &Path,
    path: &str,
    since: Option<i64>,
) -> Result<Outcome> {
    let Some(project) = new_manifest.find_project(path) else {
        return Ok(Outcome::NotInNewManifest);
    };
    let url = new_manifest.project_url(project, &args.new_manifest_url)?;
    let revision = new_manifest
        .project_revision(project)
        .ok_or_else(|| anyhow!("no revision defined for project {}", project.name))?;

    let repo_path = repo_root.join(path);
    let repo =
        Repository::open(&repo_path).with_context(|| format!("failed to open {repo_path:?}"))?;

    let baseline = collect_commits(&repo, repo.head()?.peel_to_commit()?.id(), since)?;

    let new_tip = fetch_revision(&repo_path, &url, revision, TEMP_REF)?;
    let new_commits = collect_commits(&repo, new_tip, since);
    if !args.keep_refs {
        let _ = delete_ref(&repo_path, TEMP_REF);
    }

    let missing = find_missing(&baseline, &new_commits?)
        .into_iter()
        .cloned()
        .collect();
    Ok(Outcome::Compared {
        total: baseline.len(),
        missing,
    })
}
