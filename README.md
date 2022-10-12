# repo-utils
Utilities for google's repo-tool written in Rust

A collection of commands to work on git repositories managed by google's repo tool.

## repo-forall
```
Execute commands on git repositories managed by repo, see https://github.com/elektronenhirn/repo-utils

Usage: repo-forall [OPTIONS] [COMMAND]...

Arguments:
  [COMMAND]...

Options:
  -C, --cwd <DIR>        change working directory (mostly useful for testing)
  -m, --manifest <FILE>  ignore projects which are not defined in the given manifest file(s)
  -g, --group <GROUP>    ignore projects which are not part of the given group(s)
  -v, --verbose          Verbose output, e.g. print local path before executing command
  -f, --fail-fast        Stop running commands for anymore projects whenever one failed
  -h, --help             Print help information
  -V, --version          Print version information
```

## repo-status
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
