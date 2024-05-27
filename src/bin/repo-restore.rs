extern crate clap;

use anyhow::{bail, Context, Error, Result, Ok, anyhow};
use clap::Parser;
use colored::*;
use crossbeam::channel::unbounded;
use dialoguer::Confirm;
use git2::{Repository, StatusOptions};
use indicatif::{ParallelProgressIterator, ProgressBar, ProgressStyle};
use rayon::prelude::*;
use repo_utils::repo_project_selector::{
    find_repo_manifests_folder, find_repo_root_folder, select_projects,
};
use std::convert::TryInto;
use std::env;
use std::path::PathBuf;
use std::process::{Command};
use std::str;
use std::time::Instant;

/// Restore repos managed by git-repo to the last "repo sync" state,
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

    /// Dry-run, only lists "dirty" repositories, does not take any actions
    #[arg(short, long, default_value = "false")]
    dry_run: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    if let Some(cwd) = &args.cwd {
        env::set_current_dir(cwd)?;
    }

    let list_of_projects = select_projects(false, args.group.clone(), args.manifest.clone())?;
    let cmd_context = CmdContext::from(args, list_of_projects)?;

    println!("Selected {} projects", cmd_context.list_of_projects.len());

    let dirty_repos = scan_for_dirty_repos(&cmd_context)?;

    if cmd_context.args.dry_run || dirty_repos.is_empty(){
        println!("Nothing to be done, bye");
        return Ok(());
    }

    let confirmation = Confirm::new()
    .with_prompt("DANGER: do you want to restore state from last repo sync? local-only data will be lost!")
    .interact()
    .unwrap();

    if confirmation {
        restore_dirty_repos(&cmd_context, dirty_repos)
    } else {
        println!("Skipping restoring of dirty repos");
        Ok(())
    }
}

fn scan_for_dirty_repos(cmd_context: &CmdContext) -> Result<Vec<GitStatus>> {
    let timestamp_before_scanning = Instant::now();

    // Create a simple streaming channel
    let (tx, rx) = unbounded();

    let progress_bar = ProgressBar::new(cmd_context.list_of_projects.len() as u64).with_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7}")?,
    );

    let _ = cmd_context.list_of_projects
        .par_iter()
        .progress_with(progress_bar)
        .try_for_each(|path| {
            let repo = Repository::open(cmd_context.repo_root_folder.join(&path))
                .with_context(|| format!("Failed to open git repo at {:?}", path))?;
            if repo.is_bare() {
                bail!("cannot report status on bare repository");
            }

            let statuses = repo.statuses(Some(&mut default_status_options()))?;

            let last_repo_sync_tree = repo
                .find_branch(&cmd_context.sync_branch_name, git2::BranchType::Remote)
                .map(|b| b.get().peel_to_tree())
                .with_context(|| format!("{:?}", path))??;
            let head_tree = repo
                .head()?
                .peel_to_tree()
                .with_context(|| format!("{:?}", path))?;

            let local_deltas =
                repo.diff_tree_to_tree(Some(&last_repo_sync_tree), Some(&head_tree), None)?;

            let _ = tx.send(GitStatus::new(
                path,
                !statuses.is_empty(),
                local_deltas.deltas().len().try_into().unwrap(),
            ));

            Ok(())
        })
        .expect("Querying status failed");

    let mut repo_statuses: Vec<_> = rx.try_iter().collect();
    repo_statuses.sort();

//    let repos_with_uncommited_changes = repo_statuses.iter().fold(0, |sum, gs| if gs.uncomitted_changes {sum + 1} else {sum} );
//    let repos_with_local_commits = repo_statuses.iter().fold(0, |sum, gs| if gs.local_deltas > 0 {sum + 1} else {sum} );

    let mut dirty_repos: Vec<GitStatus> = vec![];

    repo_statuses.iter().for_each(|gs| {
        if gs.uncomitted_changes || gs.local_deltas > 0 {
            dirty_repos.push(gs.clone());
        }
        gs.print(cmd_context.args.verbose);
    });

    println!();

    println!(
        "Scanning finished in {}s:\n→ {}/{} git repos deviate from the last repo sync\n",
        timestamp_before_scanning.elapsed().as_secs(),
        dirty_repos.len(),
        cmd_context.list_of_projects.len(),
    );

    Ok(dirty_repos)
}

fn restore_dirty_repos(cmd_context: &CmdContext, dirty_repos: Vec<GitStatus>) -> Result<()> {
    dirty_repos.iter().try_for_each(|v| {
        println!("Restoring {}", v.path);

        let command = format!("git clean -fd && git reset --hard {}", cmd_context.sync_branch_name);

        let output = Command::new("sh")
            .current_dir(&cmd_context.repo_root_folder.join(&v.path))
            .arg("-c")
            .arg(&command)
            .output()
            .map_err(Error::msg)?;

        match output.status.success() {
            true => Ok(()),
            false => Err(anyhow!("Failed to execute {}: {:?}", command, output.status.code())),
        }
    })?;

    println!("Restoring done");

    Ok(())
}

fn default_status_options() -> StatusOptions {
    let mut opts = StatusOptions::new();
    opts.include_ignored(false).include_untracked(true);
    opts
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone)]
struct GitStatus {
    pub path: String,
    pub uncomitted_changes: bool,
    pub local_deltas: i32,
}

impl GitStatus {
    pub fn new(path: &str, dirty: bool, local_deltas: i32) -> Self {
        GitStatus {
            path: path.to_string(),
            uncomitted_changes: dirty,
            local_deltas,
        }
    }

    pub fn print(&self, verbose: bool) {
        if self.uncomitted_changes {
            println!("{}: uncommited changes", self.path.red());
        }
        if self.local_deltas > 0 {
            println!("{}: found local commit(s)", self.path.red());
        }

        if verbose && !self.uncomitted_changes && self.local_deltas == 0 {
            println!("{}: clean", self.path.green());
        }
    }
}

// this class bundles all the objects required for the various methods in here,
// so we can pass them more conveniently into all the methods
struct CmdContext {
    sync_branch_name: String,
    repo_root_folder: PathBuf,
    args: Args,
    list_of_projects: Vec<String>,
}

impl CmdContext {
    pub fn from(args: Args, list_of_projects: Vec<String>) -> Result<CmdContext> {

        let sync_branch_name = lookup_sync_branch_name()?;
        let repo_root_folder: std::path::PathBuf = find_repo_root_folder()?;

        Ok(CmdContext{sync_branch_name, repo_root_folder, args, list_of_projects})
    }
}
// The repo tool maintains a branch tracking the last synced state
// It is typically named "m/<manifest-branch>" where manifest-branch
// is the branch used for "repo init".
fn lookup_sync_branch_name() -> Result<String> {
    // in .repo/manifests
    //git for-each-ref --format '%(upstream:lstrip=-1)' "$(git symbolic-ref -q HEAD)"

    let manifests_folder = find_repo_manifests_folder()?;

    Command::new("sh")
        .current_dir(&manifests_folder)
        .arg("-c")
        .arg("git for-each-ref --format '%(upstream:lstrip=-1)' \"$(git symbolic-ref -q HEAD)\"")
        .output()
        .map_or_else(
            |e| bail!(e),
            |o| match o.status.success() {
                true => Ok(String::from_utf8_lossy(&o.stdout).into_owned()),
                false => bail!(String::from_utf8_lossy(&o.stderr).into_owned()),
            },
        )
        .map(|s| "m/".to_string() + s.trim())
}
