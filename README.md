# repo-utils
Utilities for google's repo-tool written in Rust

A collection of commands to work on git repositories managed by google's repo tool.

## Installation

repo-utils is written in rust. You need the rust toolchain installed to be able to use it:

https://www.rust-lang.org/tools/install

Then you simply install the latest repo-utils release with:

```
cargo install repo-utils
```

from the central crates.io repository

## Usage

### repo-forall
```
Execute commands on git repositories managed by repo, see https://github.com/elektronenhirn/repo-utils

Usage: repo-forall [OPTIONS] [COMMAND]...

Arguments:
  [COMMAND]...

Options:
  -C, --cwd <DIR>           change working directory (mostly useful for testing)
  -m, --manifest <FILE>     ignore projects which are not defined in the given manifest file(s)
  -g, --group <GROUP>       ignore projects which are not part of the given group(s)
  -v, --verbose             Verbose output
  -f, --fail-fast           Stop running commands for anymore projects whenever one failed
  -p, --print-project-path  Print project path before printing command output
  -h, --help                Print help information
  -V, --version             Print version information
  ```

Supports the following environment variables in the command:

- `REPO_PATH`: relative path to the project

Example:

```
repo-forall 'echo $REPO_PATH'
```

### repo-status
```
Check if repos managed by git-repo have uncommited changes, see https://github.com/elektronenhirn/repo-utils

Usage: repo-status [OPTIONS]

Options:
  -C, --cwd <DIR>        change working directory (mostly useful for testing)
  -m, --manifest <FILE>  ignore projects which are not defined in the given manifest file(s)
  -g, --group <GROUP>    ignore projects which are not part of the given group(s)
  -v, --verbose          Verbose output, e.g. print local path before executing command
  -h, --help             Print help information
  -V, --version          Print version information
```

### repo-delta

Find commits which got lost when moving a tree to another Gerrit server. Run it inside a
checkout synced from the *old* server. For every project, the commits of the checked-out
branch are compared with the branch fetched from the *new* server (as defined by the new
server's manifest). Commits are matched by commit id or by their `Change-Id` trailer, so
slightly rewritten history is not reported. Merge commits are ignored; commits without a
Change-Id are matched by commit id only. Exits with 1 if anything is missing.

```
Usage: repo-delta [OPTIONS] --new-manifest-url <URL>

Options:
  -C, --cwd <DIR>                        change working directory (mostly useful for testing)
  -m, --manifest <FILE>                  ignore projects which are not defined in the given manifest file(s)
  -g, --group <GROUP>                    ignore projects which are not part of the given group(s)
      --new-manifest-url <URL>           URL of the manifest repository on the new server
      --new-manifest-branch <BRANCH>     branch of the new manifest repository [default: branch this checkout was initialized with]
      --new-manifest-file <FILE>         manifest file within the new manifest repository [default: default.xml]
      --since <DATE>                     only consider commits since the given date (YYYY-MM-DD)
  -j, --jobs <JOBS>                      number of projects to compare in parallel [default: 8]
      --keep-refs                        keep the temporary refs/repo-delta/new ref in each repository
  -v, --verbose                          list every missing commit
```

Note that the tool fetches from the new server into your existing repositories (into the
temporary ref `refs/repo-delta/new`, removed afterwards), so that your git credentials work.

### repo-restore

```
Restore repos managed by git-repo to the last "repo sync" state, see https://github.com/elektronenhirn/repo-utils

Usage: repo-restore [OPTIONS]

Options:
  -C, --cwd <DIR>        change working directory (mostly useful for testing)
  -m, --manifest <FILE>  ignore projects which are not defined in the given manifest file(s)
  -g, --group <GROUP>    ignore projects which are not part of the given group(s)
  -v, --verbose          Verbose output, e.g. print local path before executing command
  -d, --dry-run          Dry-run, only lists "dirty" repositories, does not take any actions
  -d, --del-git-lock     Additionally delete git .lock files
  -h, --help             Print help information
  -V, --version          Print version information
  ```