extern crate clap;

use anyhow::{bail, Context, Result};
use clap::{crate_version, App, Arg};
use colored::*;
use crossbeam::channel::unbounded;
use git2::{Repository, StatusOptions};
use indicatif::{ParallelProgressIterator, ProgressBar, ProgressStyle};
use rayon::prelude::*;
use repo_utils::repo_project_selector::{find_repo_root_folder, select_projects};
use std::env;
use std::path::Path;
use std::str;
use std::time::Instant;

fn main() -> Result<()> {
    let original_cwd = env::current_dir().expect("cwd not found");
    let cli_args = App::new("repo-status")
        .version(crate_version!())
        .author("Florian Bramer <elektronenhirn@gmail.com>")
        .about("Execute commands on git repositories managed by repo")
        .arg(
            Arg::with_name("cwd")
                .short("C")
                .long("cwd")
                .value_name("cwd")
                .help("change working directory (mostly useful for testing)")
                .default_value(original_cwd.to_str().unwrap())
                .takes_value(true),
        )
        .arg(
            Arg::with_name("filename")
                .short("m")
                .long("manifest")
                .takes_value(true)
                .multiple(true)
                .help("ignore projects which are not defined in the given manifest file(s)"),
        )
        .arg(
            Arg::with_name("groupname")
                .short("g")
                .long("group")
                .takes_value(true)
                .multiple(true)
                .help("ignore projects which are not part of the given group(s)"),
        )
        .arg(
            Arg::with_name("verbose")
                .short("v")
                .long("verbose")
                .help("Verbose output, e.g. print local path before executing command"),
        )
        .get_matches();
    let cwd = Path::new(cli_args.value_of("cwd").unwrap());
    env::set_current_dir(cwd)?;

    let list_of_projects = select_projects(
        false,
        cli_args
            .values_of("groupname")
            .map(|values| values.collect::<Vec<_>>()),
        cli_args
            .values_of("filename")
            .map(|values| values.collect::<Vec<_>>()),
    )?;

    println!("Selected {} projects", list_of_projects.len());

    status(list_of_projects, cli_args.is_present("verbose"))
}

fn status(list_of_projects: Vec<String>, verbose: bool) -> Result<()> {
    let timestamp_before_scanning = Instant::now();

    // Create a simple streaming channel
    let (tx, rx) = unbounded();
    let overall_progress = ProgressBar::new(list_of_projects.len() as u64);
    overall_progress.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7}"),
    );

    let repo_root_folder = find_repo_root_folder()?;

    let _ = list_of_projects
        .par_iter()
        .progress_with(overall_progress)
        .try_for_each(|path| {
            let repo = Repository::open(repo_root_folder.join(&path))
                .with_context(|| format!("Failed to open git repo at {:?}", path))?;
            if repo.is_bare() {
                bail!("cannot report status on bare repository");
            }

            let statuses = repo.statuses(Some(&mut default_status_options()))?;
            let _ = tx.send(GitStatus::new(&path, !statuses.is_empty()));

            Ok(())
        });

    let mut dirty = 0;
    loop {
        match rx.try_recv() {
            Err(_) => break,
            Ok(output) => {
                match output.dirty {
                    true => dirty += 1,
                    _ => (),
                }
                output.print(verbose);
            }
        }
    }

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
            println!("{}:", self.path.yellow());
        }
    }
}
