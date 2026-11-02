use repo_utils::repo_project_selector::resolve_url;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

#[test]
fn resolves_remote_urls_like_repo() {
    let resolve = resolve_url;
    assert_eq!(
        resolve("https://host/platform/manifest", ".."),
        "https://host/"
    );
    assert_eq!(
        resolve("https://host/platform/manifest", "../other"),
        "https://host/other"
    );
    assert_eq!(
        resolve("ssh://git@host:29418/platform/manifest", ".."),
        "ssh://git@host:29418/"
    );
    assert_eq!(resolve("git@host:platform/manifest", ".."), "git@host:");
    assert_eq!(
        resolve("file:///srv/manifest", "https://elsewhere/x"),
        "https://elsewhere/x"
    );
}

fn git(dir: &Path, args: &[&str], date: &str) {
    let status = Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .env("GIT_AUTHOR_DATE", date)
        .env("GIT_COMMITTER_DATE", date)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

fn commit(dir: &Path, file: &str, change_id: &str, date: &str) {
    fs::write(dir.join(file), file).unwrap();
    git(dir, &["add", file], date);
    let msg = format!("Add {file}\n\nChange-Id: I{change_id:0>40}\n");
    git(dir, &["commit", "-q", "-m", &msg], date);
}

fn init(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q", "-b", "main"], "2020-01-01T00:00:00Z");
}

/// Builds an "old" checkout with commits a,b and a "new server" whose
/// project has the rewritten commit a and the given extra commits.
fn setup(root: &Path, new_extra: &[(&str, &str)]) -> std::path::PathBuf {
    let server = root.join("server");
    let manifest = server.join("platform/manifest");
    init(&manifest);
    fs::write(
        manifest.join("default.xml"),
        r#"<manifest>
  <remote name="new" fetch=".."/>
  <default remote="new" revision="main"/>
  <project name="proj" path="proj"/>
</manifest>"#,
    )
    .unwrap();
    git(&manifest, &["add", "."], "2020-01-01T00:00:00Z");
    git(
        &manifest,
        &["commit", "-q", "-m", "m"],
        "2020-01-01T00:00:00Z",
    );

    let new_proj = server.join("proj");
    init(&new_proj);
    commit(&new_proj, "a", "a", "2021-06-01T00:00:00Z"); // same Change-Id, other sha
    for (file, id) in new_extra {
        commit(&new_proj, file, id, "2021-06-02T00:00:00Z");
    }

    let checkout = root.join("checkout");
    fs::create_dir_all(checkout.join(".repo")).unwrap();
    fs::write(checkout.join(".repo/project.list"), "proj\n").unwrap();
    let old_proj = checkout.join("proj");
    init(&old_proj);
    commit(&old_proj, "a", "a", "2020-06-01T00:00:00Z");
    commit(&old_proj, "b", "b", "2020-06-02T00:00:00Z");
    checkout
}

fn run(root: &Path, checkout: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_repo-delta"))
        .arg("-C")
        .arg(checkout)
        .arg("--new-manifest-url")
        .arg(format!(
            "file://{}/server/platform/manifest",
            root.display()
        ))
        .args(["--new-manifest-branch", "main", "-v"])
        .output()
        .unwrap()
}

#[test]
fn reports_lost_commit() {
    let tmp = tempfile::tempdir().unwrap();
    let checkout = setup(tmp.path(), &[("c", "c")]);
    let out = run(tmp.path(), &checkout);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    assert!(stdout.contains("1 of 2 commits missing"), "{stdout}");
    assert!(stdout.contains("Add b"), "{stdout}");
    assert!(!stdout.contains("Add a"), "{stdout}");
}

#[test]
fn succeeds_when_nothing_is_lost() {
    let tmp = tempfile::tempdir().unwrap();
    let checkout = setup(tmp.path(), &[("b", "b")]);
    let out = run(tmp.path(), &checkout);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{stdout}");
    assert!(stdout.contains("0 commits missing"), "{stdout}");
}

#[test]
fn project_path_defaults_to_name() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("m.xml");
    fs::write(
        &file,
        r#"<manifest><project name="a/b"/><project name="c" path="d"/></manifest>"#,
    )
    .unwrap();
    let manifest = repo_utils::repo_project_selector::parse(&file).unwrap();
    assert_eq!(manifest.projects[0].path, "a/b");
    assert_eq!(manifest.projects[1].path, "d");
}

#[test]
fn parses_manifest_with_interleaved_elements() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("m.xml");
    fs::write(
        &file,
        r#"<manifest>
  <remote name="r" fetch=".."/>
  <project name="a"><linkfile src="x" dest="y"/></project>
  <remove-project name="z"/>
  <project name="b"/>
  <!-- comment -->
  <remove-project name="y"/>
  <project name="c"/>
</manifest>"#,
    )
    .unwrap();
    let manifest = repo_utils::repo_project_selector::parse(&file).unwrap();
    let names: Vec<_> = manifest.projects.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["a", "b", "c"]);
}

#[test]
fn revision_precedence_is_project_then_remote_then_default() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("m.xml");
    fs::write(
        &file,
        r#"<manifest>
  <remote name="aosp" fetch="https://aosp"/>
  <remote name="own" fetch="https://own" revision="own-branch"/>
  <default remote="aosp" revision="refs/tags/t"/>
  <project name="a"/>
  <project name="b" remote="own"/>
  <project name="c" remote="own" revision="pinned"/>
</manifest>"#,
    )
    .unwrap();
    let m = repo_utils::repo_project_selector::parse(&file).unwrap();
    let rev = |i: usize| m.project_revision(&m.projects[i]).unwrap().to_owned();
    assert_eq!(rev(0), "refs/tags/t");
    assert_eq!(rev(1), "own-branch");
    assert_eq!(rev(2), "pinned");
}
