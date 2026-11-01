//! Command line surface for fodelta.
//!
//! This module deliberately contains only argument parsing and dispatch. All
//! behavior lives in the library modules so it can be exercised from tests.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

use crate::repo::Repository;
use crate::{ExitCode, FodeltaError, Result};

/// A local, single-branch directory version control tool.
#[derive(Debug, Parser)]
#[command(
    name = "fodelta",
    version,
    about = "A local, single-branch directory version control tool",
    propagate_version = true,
    arg_required_else_help = true
)]
pub struct Cli {
    /// Run as if fodelta was started in <DIR>.
    #[arg(short = 'C', global = true, value_name = "DIR", default_value = ".")]
    pub cwd: PathBuf,

    /// Control colored output.
    #[arg(long, global = true, value_enum, default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,

    /// Number of worker threads (defaults to the number of CPUs).
    #[arg(short = 'j', long, global = true, value_name = "N")]
    pub jobs: Option<usize>,

    /// Suppress progress output.
    #[arg(short = 'q', long, global = true)]
    pub quiet: bool,

    #[command(subcommand)]
    pub command: Command,
}

/// Color policy, mirroring the usual `auto|always|never` convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Initialize a new repository in the given directory.
    Init {
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,
    },

    /// Show working tree changes relative to HEAD.
    Status {
        /// Also list ignored, untracked paths.
        #[arg(long)]
        ignored: bool,
    },

    /// Record a snapshot of the working tree.
    Commit {
        /// Commit message.
        #[arg(short = 'm', long, value_name = "MSG")]
        message: String,

        /// Allow a commit whose tree equals HEAD.
        #[arg(long)]
        allow_empty: bool,

        /// Allow files larger than core.max_file_size_mb.
        #[arg(long)]
        allow_large_files: bool,

        /// Only commit these files or directory prefixes.
        #[arg(value_name = "PATH", num_args = 0..)]
        paths: Vec<PathBuf>,
    },

    /// Show commit history, newest first.
    Log {
        /// One line per commit.
        #[arg(long)]
        oneline: bool,

        /// Limit the number of commits.
        #[arg(short = 'n', long, value_name = "N")]
        limit: Option<usize>,

        /// Show a per-commit diffstat.
        #[arg(long)]
        stat: bool,

        /// Show oldest first.
        #[arg(long)]
        reverse: bool,

        /// Only commits touching these paths.
        #[arg(value_name = "PATH", num_args = 0..)]
        paths: Vec<PathBuf>,
    },

    /// Show a commit and its diff against its parent.
    Show {
        /// Revision, optionally suffixed with `:<path>`.
        #[arg(value_name = "REV")]
        rev: String,
    },

    /// Compare the working tree with HEAD, a revision, or two revisions.
    Diff {
        /// Zero, one or two revisions.
        #[arg(value_name = "REV", num_args = 0..=2)]
        revs: Vec<String>,

        /// Show a diffstat instead of a patch.
        #[arg(long)]
        stat: bool,

        /// Show `A/M/D/T` status lines.
        #[arg(long = "name-status")]
        name_status: bool,

        /// Show changed paths only.
        #[arg(long = "name-only")]
        name_only: bool,

        /// Number of context lines.
        #[arg(short = 'U', long = "unified", value_name = "N")]
        unified: Option<u32>,

        /// Exit with 1 when differences exist.
        #[arg(long = "exit-code")]
        exit_code: bool,

        /// Restrict the comparison to these paths.
        #[arg(last = true, value_name = "PATH")]
        paths: Vec<PathBuf>,
    },

    /// Restore paths from a revision into the working tree.
    Restore {
        /// Source revision (defaults to HEAD).
        #[arg(value_name = "REV")]
        rev: Option<String>,

        /// Remove files that do not exist in the source revision.
        #[arg(long)]
        delete: bool,

        /// Do not create a safety snapshot before overwriting changes.
        #[arg(long = "no-safety")]
        no_safety: bool,

        /// Paths to restore (defaults to the whole tree).
        #[arg(last = true, value_name = "PATH")]
        paths: Vec<PathBuf>,
    },

    /// Append a commit whose tree equals the given revision.
    Rollback {
        /// Revision to roll back to.
        #[arg(value_name = "REV")]
        rev: String,

        /// Commit message (defaults to `rollback to <short-id>`).
        #[arg(short = 'm', long, value_name = "MSG")]
        message: Option<String>,

        /// Do not create a safety snapshot before overwriting changes.
        #[arg(long = "no-safety")]
        no_safety: bool,
    },

    /// List safety snapshots.
    Safety,

    /// Inspect ignore rules.
    Ignore {
        #[command(subcommand)]
        command: IgnoreCommand,
    },

    /// Garbage collect unreachable objects.
    Gc {
        /// Only report what would be removed.
        #[arg(long = "dry-run")]
        dry_run: bool,
    },

    /// Verify repository integrity.
    Fsck {
        /// Also verify every object in the store.
        #[arg(long)]
        full: bool,
    },

    /// Read or write repository configuration.
    Config {
        #[arg(value_name = "KEY")]
        key: Option<String>,

        #[arg(value_name = "VALUE")]
        value: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum IgnoreCommand {
    /// List the `.gitignore` files in effect.
    List,

    /// Explain whether a path is ignored.
    Check {
        #[arg(value_name = "PATH")]
        path: PathBuf,
    },
}

/// Dispatch a parsed command.
pub fn dispatch(cli: Cli) -> Result<ExitCode> {
    let Cli {
        cwd,
        command,
        quiet,
        color: _,
        jobs: _,
    } = cli;
    let start = resolve_start(&cwd)?;

    match command {
        Command::Init { path } => {
            let target = match path {
                Some(path) if path.is_absolute() => path,
                Some(path) => start.join(path),
                None => start,
            };
            let repo = Repository::init(&target)?;
            if !quiet {
                println!(
                    "Initialized empty fodelta repository in {}",
                    repo.fodelta.display()
                );
            }
            Ok(ExitCode::Success)
        }
        Command::Config { key, value } => cmd_config(&start, key, value),
        Command::Status { .. } => Err(FodeltaError::NotImplemented("status")),
        Command::Commit { .. } => Err(FodeltaError::NotImplemented("commit")),
        Command::Log { .. } => Err(FodeltaError::NotImplemented("log")),
        Command::Show { .. } => Err(FodeltaError::NotImplemented("show")),
        Command::Diff { .. } => Err(FodeltaError::NotImplemented("diff")),
        Command::Restore { .. } => Err(FodeltaError::NotImplemented("restore")),
        Command::Rollback { .. } => Err(FodeltaError::NotImplemented("rollback")),
        Command::Safety => Err(FodeltaError::NotImplemented("safety")),
        Command::Ignore { .. } => Err(FodeltaError::NotImplemented("ignore")),
        Command::Gc { .. } => Err(FodeltaError::NotImplemented("gc")),
        Command::Fsck { .. } => Err(FodeltaError::NotImplemented("fsck")),
    }
}

fn resolve_start(cwd: &std::path::Path) -> Result<PathBuf> {
    let base = std::env::current_dir()?;
    let path = if cwd.is_absolute() {
        cwd.to_path_buf()
    } else {
        base.join(cwd)
    };
    Ok(path.canonicalize()?)
}

fn cmd_config(
    start: &std::path::Path,
    key: Option<String>,
    value: Option<String>,
) -> Result<ExitCode> {
    let repo = Repository::open(start)?;
    match (key, value) {
        (None, None) => {
            let text = toml::to_string_pretty(&repo.config)
                .map_err(|error| FodeltaError::Config(error.to_string()))?;
            print!("{text}");
        }
        (Some(key), None) => {
            println!("{}", repo.config.get(&key)?);
        }
        (Some(key), Some(value)) => {
            let mut config = repo.config.clone();
            config.set(&key, &value)?;
            config.save(&repo.config_path())?;
        }
        (None, Some(_)) => {
            return Err(FodeltaError::Config(
                "a configuration value requires a key".to_string(),
            ));
        }
    }
    Ok(ExitCode::Success)
}

#[cfg(test)]
mod tests {
    use clap::{CommandFactory, Parser};

    use super::*;

    #[test]
    fn debug_assert_passes() {
        Cli::command().debug_assert();
    }

    #[test]
    fn help_lists_all_commands() {
        let cmd = Cli::command();
        let names: Vec<String> = cmd
            .get_subcommands()
            .map(|sub| sub.get_name().to_string())
            .collect();
        for expected in [
            "init", "status", "commit", "log", "show", "diff", "restore", "rollback", "safety",
            "ignore", "gc", "fsck", "config",
        ] {
            assert!(
                names.iter().any(|n| n == expected),
                "missing command {expected}"
            );
        }
    }

    #[test]
    fn commit_accepts_trailing_paths() {
        let cli = Cli::try_parse_from(["fodelta", "commit", "-m", "x", "--", "a.txt", "docs/"])
            .expect("valid command line");
        match cli.command {
            Command::Commit { paths, .. } => {
                assert_eq!(paths, vec![PathBuf::from("a.txt"), PathBuf::from("docs/")]);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn diff_parses_two_revisions_and_paths() {
        let cli = Cli::try_parse_from(["fodelta", "diff", "HEAD~1", "HEAD", "--", "src/"])
            .expect("valid command line");
        match cli.command {
            Command::Diff { revs, paths, .. } => {
                assert_eq!(revs, vec!["HEAD~1".to_string(), "HEAD".to_string()]);
                assert_eq!(paths, vec![PathBuf::from("src/")]);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn restore_accepts_revision_and_paths() {
        let cli = Cli::try_parse_from(["fodelta", "restore", "HEAD~2", "--", "src/a.rs"])
            .expect("valid command line");
        match cli.command {
            Command::Restore { rev, paths, .. } => {
                assert_eq!(rev.as_deref(), Some("HEAD~2"));
                assert_eq!(paths, vec![PathBuf::from("src/a.rs")]);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }
}
