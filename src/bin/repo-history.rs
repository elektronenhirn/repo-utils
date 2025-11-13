
use anyhow::Result;
use repo_utils::repo_history::model::{MultiRepoHistory, Repo, RevWalkStrategy};
use repo_utils::repo_history::model;
use std::{env, io};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::Arc;
use repo_utils::utils::{find_project_file, find_repo_base_folder};
use repo_utils::repo_history::ui;
use clap::Parser;

// Sweet Spot? Tests on a 36 core INTEL Xeon showed that parsing becomes
// slower again if more than 18 threads are used
const MAX_NUMBER_OF_THREADS: usize = 18;

/// Shows a linear history accross all repos managed by git-repo
/// see https://github.com/elektronenhirn/repo-utils
#[derive(Parser, Debug)]
#[command(author, version, long_about = None)]
struct Args {
    /// change working directory (mostly useful for testing)
    #[arg(short = 'C', long, value_name = "DIR", value_hint = clap::ValueHint::DirPath)]
    cwd: Option<std::path::PathBuf>,

    /// ignore projects which are not defined in the given manifest file(s)
    #[arg(short = 'x', long, value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
    manifest: Option<Vec<std::path::PathBuf>>,

    /// ignore projects which are not part of the given group(s)
    #[arg(short, long)]
    group: Option<Vec<String>>,

    /// include history of the last <n> days
    #[arg(short, long, value_name = "DAYS", default_value = "365")]
    days: Option<u32>,

    /// only include commits where author's name contains <pattern> (case insensitive)
    #[arg(short, long, value_name = "AUTHOR")]
    author: Option<String>,

    /// only include commits where message contains <pattern> (case insensitive)
    #[arg(short, long, value_name = "PATTERN")]
    message: Option<String>,

    /// traverse the 1st parent only ('first' = fast) or all parents ('all' = slow)
    #[arg(short, long, value_name = "REVWALK_STRATEGY", default_value = "first")]
    revwalk_strategy: String,

    /// include changes to the manifest repository
    #[arg(short, long, default_value = "false")]
    include_manifest: bool,

    /// writes a report to a file given by <path> - supported formats: .csv, .ods, .xlsx
    #[arg(short = 'p', long, value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
    report_file_path: Option<Vec<std::path::PathBuf>>,
}



fn main() -> Result<(), String> {
    let args = Args::parse();

    let classifier = model::Classifier::new(
        args.days.unwrap_or(365),
        args.author.as_deref(),
        args.message.as_deref()
    );

    do_main(
        &classifier,
        &args.revwalk_strategy,
        args.cwd.as_deref(),
        args.include_manifest,
        args.report_file_path.as_ref().and_then(|v| v.first().map(|p| p.to_str().unwrap())),
    )
    .map_err(|e| e.to_string())
}

fn do_main(
    classifier: &model::Classifier,
    revwalk_strategy: &str,
    cwd: Option<&Path>,
    include_manifest: bool,
    report_file_path: Option<&str>,
) -> Result<()> {
    let config = repo_utils::config::Config::new();

    if let Some(cwd) = cwd {
        env::set_current_dir(cwd)?;
    }

    rayon::ThreadPoolBuilder::new()
        .num_threads(std::cmp::min(num_cpus::get(), MAX_NUMBER_OF_THREADS))
        .build_global()
        .unwrap();

    let project_file = File::open(find_project_file()?)?;
    let repos = repos_from(&project_file, include_manifest)?;

    let revwalk_enum = match revwalk_strategy {
        "first" => RevWalkStrategy::FirstParent,
        "all" => RevWalkStrategy::AllParents,
        _ => RevWalkStrategy::FirstParent,
    };
    
    let history = MultiRepoHistory::from(repos, &classifier, &revwalk_enum)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

    //TUI or report?
    match report_file_path {
        None => ui::show(history, config),
        Some(_file) => {
            println!("Report generation not yet implemented");
            // TODO: Implement report generation
            // report::generate(&history, file)?
        }
    }

    Ok(())
}

fn repos_from(
    project_file: &File,
    include_manifest: bool,
) -> Result<Vec<Arc<Repo>>, io::Error> {
    let mut repos = Vec::new();

    let base_folder = find_repo_base_folder()?;
    for project in BufReader::new(project_file).lines() {
        let rel_path = project?;
        repos.push(Arc::new(Repo::from(
            base_folder.join(&rel_path),
            rel_path,
        )));
    }

    if include_manifest {
        let rel_path = ".repo/manifests".to_owned();
        repos.push(Arc::new(Repo::from(
            base_folder.join(&rel_path),
            rel_path,
        )));
    }

    Ok(repos)
}
