extern crate clap;

use anyhow::{anyhow, bail, Error, Result};
use clap::Parser;
use colored::*;
use crossbeam::channel::unbounded;
use indicatif::{ParallelProgressIterator, ProgressBar, ProgressStyle};
use rayon::prelude::*;
use repo_utils::repo_project_selector::{find_repo_root_folder, select_projects};
use std::env;
use std::fmt;
use std::io;
use std::io::Write;
use std::process::{Command, Output};
use std::str;
use std::time::Instant;

/// Execute commands on git repositories managed by repo,
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

    /// Stop running commands for anymore projects whenever one failed
    #[arg(short, long, default_value = "false")]
    fail_fast: bool,

    command: Vec<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    if let Some(cwd) = args.cwd {
        env::set_current_dir(cwd)?;
    }

    if args.command.is_empty() {
        bail!("No command given")
    }

    let list_of_projects = select_projects(false, args.group, args.manifest)?;

    println!("Selected {} projects", list_of_projects.len());

    forall(
        list_of_projects,
        args.command.join(" "),
        args.verbose,
        args.fail_fast,
    )
}

fn forall(
    list_of_projects: Vec<String>,
    command: String,
    verbose: bool,
    fail_fast: bool,
) -> Result<()> {
    let timestamp_before_exec = Instant::now();

    let repo_root_folder = find_repo_root_folder()?;

    // Create a simple streaming channel
    let (tx, rx) = unbounded();
    let progress_bar = ProgressBar::new(list_of_projects.len() as u64).with_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7}")?,
    );

    let _ = list_of_projects
        .par_iter()
        .progress_with(progress_bar)
        .try_for_each(|path| {
            let output = CommandOutput::new(
                &path,
                Command::new("sh")
                    .current_dir(&repo_root_folder.join(path))
                    .arg("-c")
                    .arg(&command)
                    .env("REPO_PATH", path)
                    .output()
                    .map_err(Error::msg),
            );

            let result: Result<()> = match fail_fast && !&output.success() {
                true => Err(anyhow!("")),
                false => Ok(()),
            };

            let _ = tx.send(output);

            result
        });

    let (mut succeeded, mut failed) = (0, 0);

    rx.try_iter().for_each(|output| {
        match output.success() {
            true => succeeded += 1,
            false => failed += 1,
        }
        output.print(verbose);
    });

    println!();

    match failed {
        0 => {
            println!(
                "Finished in {}s: {}/{} executions succeeded, {} failed",
                timestamp_before_exec.elapsed().as_secs(),
                succeeded,
                list_of_projects.len(),
                failed
            );
            Ok(())
        }
        _ => Err(anyhow!(
            "Finished in {}s: {} executions failed, {}/{} succeeded",
            timestamp_before_exec.elapsed().as_secs(),
            failed,
            succeeded,
            list_of_projects.len()
        )),
    }
}

struct CommandOutput {
    pub path: String,
    pub output: Result<Output>,
}

impl CommandOutput {
    pub fn new(path: &str, output: Result<Output>) -> Self {
        CommandOutput {
            path: path.to_string(),
            output,
        }
    }

    pub fn success(&self) -> bool {
        match &self.output {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }

    pub fn print(&self, verbose: bool) {
        if !self.success() {
            //when command failed, always print local path
            println!("{}:", self.path.red());
        } else if verbose {
            println!("{}:", self.path.yellow());
        }
        match &self.output {
            Ok(output) => {
                let _ = io::stdout().write_all(&output.stdout);
                let _ = io::stdout().write_all(&output.stderr);
            }
            Err(e) => {
                eprintln!("Failed to execute given command: {}", e);
            }
        }
    }
}

impl fmt::Display for CommandOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}:", self.path)?;
        match &self.output {
            Ok(output) => {
                writeln!(
                    f,
                    "{}",
                    str::from_utf8(&output.stdout).expect("failed to convert output into string")
                )?;
                writeln!(
                    f,
                    "{}",
                    str::from_utf8(&output.stderr).expect("failed to convert output into string")
                )
            }
            Err(e) => writeln!(f, "Failed to execute given command: {}", e),
        }
    }
}
