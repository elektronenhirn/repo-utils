extern crate clap;

use anyhow::{anyhow, Error, Result};
use clap::{crate_version, App, Arg};
use colored::*;
use crossbeam::channel::unbounded;
use indicatif::{ParallelProgressIterator, ProgressBar, ProgressStyle};
use rayon::prelude::*;
use repo_utils::repo_project_selector::{find_repo_root_folder, select_projects};
use std::env;
use std::fmt;
use std::io;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output};
use std::str;

fn main() -> Result<()> {
    let original_cwd = env::current_dir().expect("cwd not found");
    let cli_args = App::new("repo-for-all")
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
            Arg::with_name("command")
                .help("The command line to execute on each selected project")
                .multiple(true)
                .required(true)
                .index(1),
        )
        .arg(
            Arg::with_name("verbose")
                .short("v")
                .long("verbose")
                .help("Verbose output, e.g. print local path before executing command"),
        )
        .arg(
            Arg::with_name("fail-fast")
                .short("f")
                .long("fail-fast")
                .help("Stop running commands for upcoming projects whenever one failed"),
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

    forall(
        list_of_projects,
        cli_args
            .values_of("command")
            .expect("command given")
            .collect::<Vec<_>>()
            .join(" "),
        cli_args.is_present("verbose"),
        cli_args.is_present("fail-fast"),
    )
}

fn forall(
    list_of_projects: Vec<String>,
    command: String,
    verbose: bool,
    fail_fast: bool,
) -> Result<()> {
    let repo_root_folder = find_repo_root_folder()?;

    // Create a simple streaming channel
    let (tx, rx) = unbounded();
    let overall_progress = ProgressBar::new(list_of_projects.len() as u64);
    overall_progress.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7}"),
    );

    let _ = list_of_projects
        .par_iter()
        .progress_with(overall_progress)
        .try_for_each(|path| {
            let output = CommandOutput::new(
                &path,
                Command::new("sh")
                    .current_dir(&repo_root_folder.join(path))
                    .arg("-c")
                    .arg(&command)
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
    loop {
        match rx.try_recv() {
            Err(_) => break,
            Ok(output) => {
                match output.success() {
                    true => succeeded += 1,
                    false => failed += 1,
                }
                output.print(verbose);
            }
        }
    }

    println!();

    match failed {
        0 => {
            println!(
                "Done: {}/{} executions succeeded, {} failed",
                succeeded,
                list_of_projects.len(),
                failed
            );
            Ok(())
        }
        _ => Err(anyhow!(
            "{} executions failed, {}/{} succeeded",
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
