use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use colored::*;
use crossbeam::channel::unbounded;
use git2::{Repository, StatusOptions};
use indicatif::{ParallelProgressIterator, ProgressBar, ProgressStyle};
use rayon::prelude::*;
use repo_utils::repo_project_selector::{find_repo_root_folder, select_projects, find_repo_manifests_folder};
use std::{convert::TryInto, env, process::Command, str, time::Instant};

/// Check if repos managed by git-repo have local-only or uncommited changes,
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

    /// Verbose output, e.g. print local path before executing command
    #[arg(short, long, default_value = "false")]
    verbose: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    if let Some(cwd) = &args.cwd {
        env::set_current_dir(cwd)?;
    }

    let list_of_projects = select_projects(false, args.group, args.manifest)?;

    println!("Selected {} projects", list_of_projects.len());

    status(list_of_projects, args.verbose)
}

fn status(list_of_projects: Vec<String>, verbose: bool) -> Result<()> {
    let timestamp_before_scanning = Instant::now();

    let sync_branch_name = lookup_sync_branch_name()?;

    // Create a simple streaming channel
    let (tx, rx) = unbounded();

    let progress_bar = ProgressBar::new(list_of_projects.len() as u64).with_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7}")?,
    );

    let repo_root_folder = find_repo_root_folder()?;

    let _ = list_of_projects
        .par_iter()
        .progress_with(progress_bar)
        .try_for_each(|path| {
            let repo = Repository::open(repo_root_folder.join(&path))
                .with_context(|| format!("Failed to open git repo at {:?}", path))?;
            if repo.is_bare() {
                bail!("cannot report status on bare repository");
            }

            let statuses = repo.statuses(Some(&mut default_status_options()))?;
            
            let last_repo_sync_tree = repo
                .find_branch(&sync_branch_name, git2::BranchType::Remote)
                .map_err(|e| anyhow!("Failed to find branch: {}", e))
                .and_then(|b| b.get().peel_to_tree().map_err(Into::into))
                .with_context(|| format!("{:?}", path))?;
            let head_tree = repo.head()?.peel_to_tree().with_context(|| format!("{:?}", path))?;

            let local_commits = repo.diff_tree_to_tree(
                Some(&last_repo_sync_tree),
                Some(&head_tree),
                None
            )?;

            let _ = tx.send(GitStatus::new(path, !statuses.is_empty(), local_commits.deltas().len().try_into().unwrap()));

            Ok(())
        })
        .expect("Querying status failed");

    let mut dirty = 0;
    let mut local_commits = 0;
    let mut repo_statuses: Vec<_> = rx.try_iter().collect();
    repo_statuses.sort();

    for status in &repo_statuses {
        if status.uncomitted_changes {
            dirty += 1;
        }
        if status.local_commits > 0 {
            local_commits += 1;
        }
        status.print(verbose);
    }

    println!();

    println!(
        "Finished in {}s: {}+{}/{} git repos dirty",
        timestamp_before_scanning.elapsed().as_secs(),
        dirty,
        local_commits,
        list_of_projects.len(),
    );

    Ok(())
}

fn default_status_options() -> StatusOptions {
    let mut opts = StatusOptions::new();
    opts.include_ignored(false).include_untracked(true);
    opts
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct GitStatus {
    pub path: String,
    pub uncomitted_changes: bool,
    pub local_commits: i32
}

impl GitStatus {
    pub fn new(path: &str, dirty: bool, local_commits: i32) -> Self {
        Self {
            path: path.to_owned(),
            uncomitted_changes: dirty,
            local_commits,
        }
    }

    pub fn print(&self, verbose: bool) {
        if self.uncomitted_changes {
            println!("{}: uncommited changes", self.path.red());
        }
        if self.local_commits > 0 {
            println!("{}: {} local commits", self.path.red(), self.local_commits);
        } 
        
        if verbose && !self.uncomitted_changes && self.local_commits == 0 {
            println!("{}: clean", self.path.green());
        }
    }
}

// The repo tool maintains a branch tracking the last synced state
// It is typically named "m/<manifest-branch>" where manifest-branch
// is the branch used for "repo init".
fn lookup_sync_branch_name() -> Result<String> {
    // in .repo/manifests
    //git for-each-ref --format '%(upstream:lstrip=-1)' "$(git symbolic-ref -q HEAD)"

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

