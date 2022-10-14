extern crate clap;

use anyhow::{bail, Context, Result};
use clap::Parser;
use colored::*;
use crossbeam::channel::unbounded;
use git2::{Repository, StatusOptions};
use indicatif::{ParallelProgressIterator, ProgressBar, ProgressStyle};
use rayon::prelude::*;
use repo_utils::repo_project_selector::{find_repo_root_folder, select_projects};
use std::env;
use std::str;
use std::time::Instant;

/// Check if repos managed by git-repo have uncommited changes,
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

    if let Some(cwd) = args.cwd {
        env::set_current_dir(cwd)?;
    }

    let list_of_projects = select_projects(false, args.group, args.manifest)?;

    println!("Selected {} projects", list_of_projects.len());

    status(list_of_projects, args.verbose)
}

fn status(list_of_projects: Vec<String>, verbose: bool) -> Result<()> {
    let timestamp_before_scanning = Instant::now();

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
            let _ = tx.send(GitStatus::new(path, !statuses.is_empty()));

            Ok(())
        });

    let mut dirty = 0;
    let mut repo_statuses: Vec<_> = rx.try_iter().collect();
    repo_statuses.sort();

    repo_statuses.iter().for_each(|v| {
        if v.dirty {
            dirty += 1;
        }
        v.print(verbose);
    });

    println!();

    println!(
        "Finished in {}s: {}/{} git repos dirty",
        timestamp_before_scanning.elapsed().as_secs(),
        dirty,
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
    pub dirty: bool,
}

impl GitStatus {
    pub fn new(path: &str, dirty: bool) -> Self {
        GitStatus {
            path: path.to_string(),
            dirty,
        }
    }

    pub fn print(&self, verbose: bool) {
        if self.dirty {
            //when command failed, always print local path
            println!("{}: dirty", self.path.red());
        } else if verbose {
            println!("{}: clean", self.path.green());
        }
    }
}
