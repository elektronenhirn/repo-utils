# repo-utils
Utilities for google's repo-tool written in Rust

A collection of commands to work on git repositories managed by google's repo tool.

## repo-forall
```
Execute commands on git repositories managed by repo

USAGE:
    repo-forall [FLAGS] [OPTIONS] <command>...

FLAGS:
    -f, --fail-fast    Stop running commands for upcoming projects whenever one failed
    -h, --help         Prints help information
    -V, --version      Prints version information
    -v, --verbose      Verbose output, e.g. print local path before executing command

OPTIONS:
    -C, --cwd <cwd>                 change working directory (mostly useful for testing)
    -m, --manifest <filename>...    ignore projects which are not defined in the given manifest file(s)
    -g, --group <groupname>...      ignore projects which are not part of the given group(s)

ARGS:
    <command>...    The command line to execute on each selected project
```

## repo-status
```
Check if repos managed by git-repo have uncommited changes

USAGE:
    repo-status [FLAGS] [OPTIONS]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information
    -v, --verbose    Verbose output, e.g. print local path before executing command

OPTIONS:
    -C, --cwd <cwd>                 change working directory (mostly useful for testing)
    -m, --manifest <filename>...    ignore projects which are not defined in the given manifest file(s)
    -g, --group <groupname>...      ignore projects which are not part of the given group(s)
```
