use crate::utils::{as_datetime, as_datetime_utc};
use chrono::{Datelike, Duration, Timelike};
use dialoguer::console::style;
use git2::{Commit, Oid, Repository, Time};
use indicatif::{MultiProgress, ParallelProgressIterator, ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// A history of commits across multiple repositories
pub struct MultiRepoHistory {
    pub repos: Vec<Arc<Repo>>,
    pub commits: Vec<RepoCommit>,
    pub locally_missing_commits: usize,
}

impl MultiRepoHistory {
    pub fn from(
        repos: Vec<Arc<Repo>>,
        classifier: &Classifier,
        rewalk_strategy: &RevWalkStrategy,
    ) -> Result<MultiRepoHistory, git2::Error> {
        let (progress, progress_bars, overall_progress) = Self::create_progress_bars(&repos);

        let missing_commits = Arc::new(AtomicUsize::new(0));
        let missing_commits_result = missing_commits.clone();

        let mut commits: Vec<RepoCommit> = repos
            .par_iter()
            .flat_map(move |repo| {
                let progress_bar = &progress_bars[rayon::current_thread_index().unwrap_or(0)];
                progress_bar.set_message(format!("Scanning {}", repo.rel_path));

                let progress_error = |msg: &str, error: &dyn std::error::Error| {
                    progress_bar.println(format!(
                        "{}: {}: {}",
                        style(&msg).red(),
                        style(&repo.rel_path).blue(),
                        error
                    ));
                    progress_bar.inc(1);
                    progress_bar.set_message("Idle");
                };

                let git_repo = match Repository::open(&repo.abs_path) {
                    Ok(repo) => repo,
                    Err(e) => {
                        progress_error("Failed to open", &e);
                        return Vec::new();
                    }
                };

                let mut revwalk = match git_repo.revwalk() {
                    Ok(revwalk) => revwalk,
                    Err(e) => {
                        progress_error("Failed create revwalk", &e);
                        return Vec::new();
                    }
                };

                if let Err(e) = revwalk.push_head() {
                    progress_error("Failed query history", &e);
                    return Vec::new();
                }
                
                if rewalk_strategy == &RevWalkStrategy::FirstParent {
                    let _ = revwalk.simplify_first_parent();
                }
                let _ = revwalk.set_sorting(git2::Sort::TIME);

                let mut commits = Vec::new();
                for commit_id in revwalk {
                    let commit = match commit_id.and_then(|commit_id| git_repo.find_commit(commit_id)) {
                        Ok(commit) => commit,
                        Err(_e) => {
                            missing_commits.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                            continue;
                        }
                    };
                    let (include, abort) = classifier.classify(&commit);
                    if include {
                        commits.push(RepoCommit::from(repo.clone(), &commit));
                    }
                    if abort {
                        break;
                    }
                }
                progress_bar.set_message("Idle");
                commits
            })
            .progress_with(overall_progress)
            .collect();

        commits.sort_unstable_by(|a, b| a.commit_time.cmp(&b.commit_time).reverse());

        progress.clear().unwrap();

        Ok(MultiRepoHistory {
            repos,
            commits,
            locally_missing_commits: missing_commits_result.load(Ordering::Relaxed),
        })
    }

    fn create_progress_bars(
        repos: &Vec<Arc<Repo>>,
    ) -> (MultiProgress, Vec<ProgressBar>, ProgressBar) {
        let progress = MultiProgress::new();
        let progress_bars = (0..rayon::current_num_threads())
            .enumerate()
            .map(|(n, _)| {
                let pb = ProgressBar::hidden()
                .with_prefix(n.to_string())
                .with_style(
                    ProgressStyle::default_spinner().template("[{prefix}] {wide_msg:.bold.dim}").expect("Valid template"),
                );
                progress.add(pb)
            })
            .collect::<Vec<ProgressBar>>();
        let overall_progress = ProgressBar::new(repos.len() as u64);
        overall_progress.set_style(
            ProgressStyle::default_bar()
                .template(" {spinner:.bold.cyan}  Scanned {pos} of {len} repositories").expect("Valid template"),
        );
        let overall_progress = progress.add(overall_progress);
        (progress, progress_bars, overall_progress)
    }
}

impl fmt::Debug for MultiRepoHistory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        println!("Commits: {}", self.commits.len());
        for commit in &self.commits {
            write!(f, "{:?}", commit)?;
        }
        Ok(())
    }
}

/// representation of a local git repository
pub struct Repo {
    pub abs_path: PathBuf,
    pub rel_path: String,
    pub description: String,
}

impl Repo {
    pub fn from(abs_path: PathBuf, rel_path: String) -> Self {
        let description = abs_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&rel_path)
            .to_owned();
        Self {
            abs_path,
            rel_path,
            description,
        }
    }
}

/// representation of a git commit associated
/// with a local git repository
#[derive(Clone)]
pub struct RepoCommit {
    pub repo: Arc<Repo>,
    pub commit_time: Time,
    pub summary: String,
    pub author: String,
    pub committer: String,
    pub commit_id: Oid,
    pub message: String,
}

impl RepoCommit {
    pub fn from(repo: Arc<Repo>, commit: &Commit) -> Self {
        Self {
            repo,
            commit_time: commit.time(),
            summary: commit.summary().unwrap_or("None").to_owned(),
            author: commit.author().name().unwrap_or("None").to_owned(),
            committer: commit.committer().name().unwrap_or("None").to_owned(),
            commit_id: commit.id(),
            message: commit.message().unwrap_or("").to_owned(),
        }
    }

    pub fn time_as_str(&self) -> String {
        let date_time = as_datetime(&self.commit_time);
        let offset = Duration::seconds(i64::from(date_time.offset().local_minus_utc()));

        format!(
            "{:04}-{:02}-{:02} {:02}:{:02} {:+02}{:02}",
            date_time.year(),
            date_time.month(),
            date_time.day(),
            date_time.hour(),
            date_time.minute(),
            offset.num_hours(),
            offset.num_minutes() - offset.num_hours() * 60
        )
    }
}

impl fmt::Debug for RepoCommit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{} {:10.10} {:10.10} {}",
            self.time_as_str(),
            self.repo.description,
            self.committer,
            self.summary
        )
    }
}

pub struct Classifier {
    age: u32,
    author: Option<String>,
    message: Option<String>,
}

impl Classifier {
    pub fn new(age: u32, author: Option<&str>, message: Option<&str>) -> Self {
        Self {
            age,
            author: author.map(str::to_lowercase),
            message: message.map(str::to_lowercase),
        }
    }
}

impl Classifier {
    fn classify(&self, commit: &Commit) -> (bool, bool) {
        let utc = as_datetime_utc(&commit.time());
        let diff = chrono::Utc::now().signed_duration_since(utc);
        let include = diff.num_days() as u32 <= self.age;
        let (mut include, abort) = (include, !include);

        if let Some(ref message) = self.message {
            let commit_message = commit.message().unwrap_or("").to_ascii_lowercase();
            include &= commit_message.contains(message);
        }

        if let Some(ref author) = self.author {
            let commit_author = commit.author().name().unwrap_or("").to_ascii_lowercase();
            include &= commit_author.contains(author);
        }

        (include, abort)
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum RevWalkStrategy {
    FirstParent,
    AllParents,
}
