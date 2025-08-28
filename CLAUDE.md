# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

repo-utils is a Rust project that provides utilities for Google's repo tool. It contains three main binary commands (`repo-forall`, `repo-status`, `repo-restore`) that help manage git repositories within repo-managed projects.

## Development Commands

### Build
```bash
cargo build
```

### Test
```bash
cargo test
```

### Run specific binary
```bash
cargo run --bin repo-forall -- [args]
cargo run --bin repo-status -- [args] 
cargo run --bin repo-restore -- [args]
```

### Install locally
```bash
cargo install --path .
```

## Architecture

The project follows a standard Rust workspace structure:

- **src/lib.rs**: Exports the `repo_project_selector` module
- **src/repo_project_selector.rs**: Core library containing project selection logic, manifest parsing, and repo discovery functions
- **src/bin/**: Contains three binary executables:
  - `repo-forall.rs`: Execute commands across multiple repo projects
  - `repo-status.rs`: Check status of repo-managed projects
  - `repo-restore.rs`: Restore projects to clean state

### Key Components

- **Project Selection**: The `select_projects()` function filters repo projects by groups and manifest files
- **Manifest Parsing**: XML parsing of repo manifest files using serde-xml-rs
- **Parallel Execution**: Uses rayon for parallel processing across repositories
- **Progress Indication**: indicatif crate for progress bars during operations

The binaries share common functionality through the library crate, particularly for project discovery and filtering based on repo's `.repo/project.list` and manifest files.