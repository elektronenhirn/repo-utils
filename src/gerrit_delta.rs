use anyhow::{Context, Result, bail};
use git2::{Oid, Repository, Sort};
use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

/// A (non-merge) commit as relevant for comparing two Gerrit servers
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitInfo {
    pub oid: Oid,
    pub change_id: Option<String>,
    pub summary: String,
}

/// Extracts the Gerrit Change-Id from the trailer block (the last
/// paragraph) of a commit message.
pub fn parse_change_id(message: &str) -> Option<String> {
    let trailers = message.trim_end().rsplit("\n\n").next()?;
    trailers
        .lines()
        .rev()
        .filter_map(|line| line.strip_prefix("Change-Id:"))
        .map(str::trim)
        .find(|id| id.len() == 41 && id.starts_with('I') && id[1..].bytes().all(is_lower_hex))
        .map(str::to_owned)
}

fn is_lower_hex(b: u8) -> bool {
    b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
}

/// Lists all non-merge commits reachable from `start`; if `since`
/// (a unix timestamp) is given, commits older than that are ignored.
pub fn collect_commits(
    repo: &Repository,
    start: Oid,
    since: Option<i64>,
) -> Result<Vec<CommitInfo>> {
    let mut walk = repo.revwalk()?;
    walk.set_sorting(Sort::TIME)?;
    walk.push(start)?;

    let mut commits = Vec::new();
    for oid in walk {
        let commit = repo.find_commit(oid?)?;
        if since.is_some_and(|since| commit.time().seconds() < since) {
            continue;
        }
        if commit.parent_count() > 1 {
            continue;
        }
        commits.push(CommitInfo {
            oid: commit.id(),
            change_id: commit.message().ok().and_then(parse_change_id),
            summary: commit
                .summary()
                .ok()
                .flatten()
                .unwrap_or_default()
                .to_owned(),
        });
    }
    Ok(commits)
}

/// Returns the commits of `baseline` which can't be found in `other`,
/// neither by commit id nor by Change-Id.
pub fn find_missing<'a>(baseline: &'a [CommitInfo], other: &[CommitInfo]) -> Vec<&'a CommitInfo> {
    let oids: HashSet<Oid> = other.iter().map(|c| c.oid).collect();
    let change_ids: HashSet<&str> = other
        .iter()
        .filter_map(|c| c.change_id.as_deref())
        .collect();

    baseline
        .iter()
        .filter(|c| !oids.contains(&c.oid))
        .filter(|c| {
            c.change_id
                .as_deref()
                .is_none_or(|id| !change_ids.contains(id))
        })
        .collect()
}

/// Fetches `revision` (branch, tag or sha) from `url` into the given
/// local ref of the repository at `repo_path` and returns the id it
/// points to. Shells out to git, so the user's ssh and credential
/// setup is honored.
pub fn fetch_revision(repo_path: &Path, url: &str, revision: &str, local_ref: &str) -> Result<Oid> {
    let source = if revision.starts_with("refs/") || is_sha(revision) {
        revision.to_owned()
    } else {
        format!("refs/heads/{revision}")
    };

    let output = Command::new("git")
        .current_dir(repo_path)
        .args(["fetch", "--quiet", "--no-tags", url])
        .arg(format!("+{source}:{local_ref}"))
        .output()
        .context("failed to execute git")?;
    if !output.status.success() {
        bail!(
            "git fetch from {url} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Repository::open(repo_path)?
        .refname_to_id(local_ref)
        .map_err(Into::into)
}

/// Removes a ref created by `fetch_revision`
pub fn delete_ref(repo_path: &Path, local_ref: &str) -> Result<()> {
    Repository::open(repo_path)?
        .find_reference(local_ref)?
        .delete()
        .map_err(Into::into)
}

fn is_sha(revision: &str) -> bool {
    revision.len() == 40 && revision.bytes().all(is_lower_hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "I0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn change_id_from_trailer() {
        let msg = format!(
            "Fix the thing\n\nLonger text.\n\nBug: 1\nChange-Id: {ID}\nSigned-off-by: A <a@b>\n"
        );
        assert_eq!(parse_change_id(&msg).as_deref(), Some(ID));
    }

    #[test]
    fn change_id_absent_or_not_in_trailer() {
        assert_eq!(parse_change_id("Just a subject"), None);
        let msg = format!("Subject\n\nChange-Id: {ID}\n\nmore body after it");
        assert_eq!(parse_change_id(&msg), None);
        assert_eq!(parse_change_id("Subject\n\nChange-Id: Inothex\n"), None);
    }
}
