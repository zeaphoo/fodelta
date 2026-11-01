//! Command line surface for fodelta.
//!
//! This module deliberately contains only argument parsing and dispatch. All
//! behavior lives in the library modules so it can be exercised from tests.

use std::io::IsTerminal;
use std::path::PathBuf;

use clap::{CommandFactory, Parser, Subcommand, ValueEnum};

use crate::diff::render::{RenderOptions, render as render_diff};
use crate::diff::text::WhitespaceOptions;
use crate::diff::{
    DiffAlgorithm, FileChange, FileChangeKind, RenameOptions, detect_renames, diff_trees,
    diff_trees_with, head_side, rev_side, worktree_side,
};
use crate::history::{
    CommitOptions, LogOptions, RollbackOptions, ShowOutput, create_commit, log_entries,
    rollback as rollback_history, show,
};
use crate::ignore as ignore_rules;
use crate::maintenance::{fsck as fsck_repo, gc as gc_repo};
use crate::repo::Repository;
use crate::safety as safety_store;
use crate::util::path;
use crate::util::time;
use crate::worktree::compute_status;
use crate::worktree::restore::{RestoreOptions, restore as restore_worktree};
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

        /// Ignore all whitespace when comparing lines.
        #[arg(short = 'w', long = "ignore-all-space")]
        ignore_all_space: bool,

        /// Ignore changes in the amount of whitespace.
        #[arg(short = 'b', long = "ignore-space-change")]
        ignore_space_change: bool,

        /// Ignore whitespace at end of lines.
        #[arg(long = "ignore-space-at-eol")]
        ignore_space_at_eol: bool,

        /// Diff algorithm: myers, patience or histogram.
        #[arg(
            long = "diff-algorithm",
            value_name = "ALG",
            value_parser = ["myers", "patience", "histogram"]
        )]
        algorithm: Option<String>,

        /// Detect renames (default) with the default threshold.
        #[arg(short = 'M', long = "find-renames")]
        find_renames: bool,

        /// Rename similarity threshold (1-100, default 50).
        #[arg(
            long = "rename-threshold",
            value_name = "N",
            value_parser = clap::value_parser!(u8).range(1..=100)
        )]
        rename_threshold: Option<u8>,

        /// Disable rename detection.
        #[arg(long = "no-renames")]
        no_renames: bool,

        /// Only show changes matching these status letters (A/D/M/R/T).
        #[arg(long = "diff-filter", value_name = "FILTER")]
        diff_filter: Option<String>,

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

    /// Generate shell completions (hidden helper command).
    #[command(hide = true)]
    Completion {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
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
        color,
        jobs,
    } = cli;

    let color_enabled = match color {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => {
            std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
        }
    };
    crate::output::set_enabled(color_enabled);
    if let Some(jobs) = jobs {
        if jobs == 0 {
            return Err(FodeltaError::Config(
                "--jobs must be at least 1".to_string(),
            ));
        }
        let _ = rayon::ThreadPoolBuilder::new()
            .num_threads(jobs)
            .build_global();
    }

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
        Command::Status { ignored } => cmd_status(&start, ignored),
        Command::Commit {
            message,
            allow_empty,
            allow_large_files,
            paths,
        } => cmd_commit(&start, message, allow_empty, allow_large_files, &paths),
        Command::Log {
            oneline,
            limit,
            stat,
            reverse,
            paths,
        } => cmd_log(&start, oneline, limit, stat, reverse, &paths),
        Command::Show { rev } => cmd_show(&start, &rev),
        Command::Diff {
            revs,
            stat,
            name_status,
            name_only,
            unified,
            exit_code,
            ignore_all_space,
            ignore_space_change,
            ignore_space_at_eol,
            algorithm,
            find_renames,
            rename_threshold,
            no_renames,
            diff_filter,
            paths,
        } => cmd_diff(
            &start,
            &revs,
            stat,
            name_status,
            name_only,
            unified,
            exit_code,
            ignore_all_space,
            ignore_space_change,
            ignore_space_at_eol,
            algorithm,
            find_renames,
            rename_threshold,
            no_renames,
            diff_filter,
            &paths,
        ),
        Command::Restore {
            rev,
            delete,
            no_safety,
            paths,
        } => cmd_restore(&start, rev, delete, no_safety, &paths),
        Command::Rollback {
            rev,
            message,
            no_safety,
        } => cmd_rollback(&start, &rev, message, no_safety),
        Command::Safety => cmd_safety(&start),
        Command::Ignore { command } => cmd_ignore(&start, command),
        Command::Completion { shell } => {
            let mut command = Cli::command();
            clap_complete::generate(shell, &mut command, "fodelta", &mut std::io::stdout());
            Ok(ExitCode::Success)
        }
        Command::Gc { dry_run } => cmd_gc(&start, dry_run),
        Command::Fsck { full } => cmd_fsck(&start, full),
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

fn cmd_restore(
    start: &std::path::Path,
    rev: Option<String>,
    delete: bool,
    no_safety: bool,
    paths: &[PathBuf],
) -> Result<ExitCode> {
    let repo = Repository::open(start)?;
    let source = match rev {
        Some(rev) => Some(crate::history::resolve(&repo, &rev)?),
        None => None,
    };
    let specs = resolve_specs(&repo, start, paths)?;
    let outcome = restore_worktree(
        &repo,
        &RestoreOptions {
            source,
            paths: specs,
            delete,
            no_safety,
        },
    )?;

    if let Some(safety) = outcome.safety {
        println!("safety snapshot {}", safety.short());
    }
    if outcome.safety_skipped {
        eprintln!("warning: no safety snapshot was created; uncommitted changes may be lost");
    }
    println!(
        "restored {} file(s), deleted {} file(s)",
        outcome.restored, outcome.deleted
    );
    Ok(ExitCode::Success)
}

fn cmd_rollback(
    start: &std::path::Path,
    rev: &str,
    message: Option<String>,
    no_safety: bool,
) -> Result<ExitCode> {
    let repo = Repository::open(start)?;
    let target = crate::history::resolve(&repo, rev)?;
    let display_message = message
        .clone()
        .unwrap_or_else(|| format!("rollback to {}", target.short()));
    let outcome = rollback_history(
        &repo,
        &RollbackOptions {
            target,
            message,
            no_safety,
        },
    )?;

    match outcome.commit {
        Some(oid) => {
            println!("[main {}] {display_message}", oid.short());
            println!(" note: rollback is append-only; previous tip is HEAD~1");
        }
        None => println!("nothing to rollback"),
    }
    if let Some(safety) = outcome.safety {
        println!("safety snapshot {}", safety.short());
    }
    if outcome.safety_skipped {
        eprintln!("warning: no safety snapshot was created; uncommitted changes may be lost");
    }
    if outcome.synced {
        if outcome.changed > 0 {
            println!(" {} file(s) changed", outcome.changed);
        } else {
            println!("working tree synchronized");
        }
    }
    Ok(ExitCode::Success)
}

fn cmd_gc(start: &std::path::Path, dry_run: bool) -> Result<ExitCode> {
    let repo = Repository::open(start)?;
    let outcome = gc_repo(&repo, dry_run)?;
    println!("reachable objects: {}", outcome.reachable);
    println!("dangling objects:  {}", outcome.dangling);
    if dry_run {
        println!(
            "would remove: {} object(s), {} byte(s)",
            outcome.removed, outcome.bytes
        );
    } else {
        println!(
            "removed: {} object(s), {} byte(s)",
            outcome.removed, outcome.bytes
        );
    }
    Ok(ExitCode::Success)
}

fn cmd_fsck(start: &std::path::Path, full: bool) -> Result<ExitCode> {
    let repo = Repository::open(start)?;
    let report = fsck_repo(&repo, full)?;
    println!(
        "checked {} object(s), {} root(s), {} dangling",
        report.checked, report.roots, report.dangling
    );
    for problem in &report.problems {
        eprintln!("fsck: {problem}");
    }
    if report.is_healthy() {
        Ok(ExitCode::Success)
    } else {
        eprintln!("fsck: {} problem(s) found", report.problems.len());
        Ok(ExitCode::Error)
    }
}

fn cmd_safety(start: &std::path::Path) -> Result<ExitCode> {
    let repo = Repository::open(start)?;
    let entries = safety_store::list(&repo)?;
    if entries.is_empty() {
        println!("no safety snapshots");
    } else {
        for entry in entries {
            println!(
                "{}  {}  {}",
                entry.oid.short(),
                time::format_timestamp(entry.timestamp, entry.offset_minutes),
                entry.reason
            );
        }
    }
    Ok(ExitCode::Success)
}

#[allow(clippy::too_many_arguments)]
fn cmd_diff(
    start: &std::path::Path,
    revs: &[String],
    stat: bool,
    name_status: bool,
    name_only: bool,
    unified: Option<u32>,
    exit_code: bool,
    ignore_all_space: bool,
    ignore_space_change: bool,
    ignore_space_at_eol: bool,
    algorithm: Option<String>,
    find_renames: bool,
    rename_threshold: Option<u8>,
    no_renames: bool,
    diff_filter: Option<String>,
    paths: &[PathBuf],
) -> Result<ExitCode> {
    let repo = Repository::open(start)?;
    if no_renames && find_renames {
        return Err(FodeltaError::Config(
            "--no-renames and --find-renames are mutually exclusive".to_string(),
        ));
    }
    if no_renames && rename_threshold.is_some() {
        return Err(FodeltaError::Config(
            "--no-renames cannot be combined with --rename-threshold".to_string(),
        ));
    }
    let filter = match diff_filter {
        Some(filter) => {
            let upper = filter.to_ascii_uppercase();
            if upper.is_empty() || !upper.chars().all(|letter| "ADMRT".contains(letter)) {
                return Err(FodeltaError::Config(
                    "--diff-filter accepts A, D, M, R and T".to_string(),
                ));
            }
            Some(upper)
        }
        None => None,
    };
    let selected = [stat, name_status, name_only]
        .iter()
        .filter(|value| **value)
        .count();
    if selected > 1 {
        return Err(FodeltaError::Config(
            "--stat, --name-status and --name-only are mutually exclusive".to_string(),
        ));
    }

    let specs = resolve_specs(&repo, start, paths)?;
    let (left, right) = match revs {
        [] => (head_side(&repo)?, worktree_side(&repo)?),
        [rev] => {
            let oid = crate::history::resolve(&repo, rev)?;
            (rev_side(&repo, oid)?, worktree_side(&repo)?)
        }
        [first, second] => {
            let first = crate::history::resolve(&repo, first)?;
            let second = crate::history::resolve(&repo, second)?;
            (rev_side(&repo, first)?, rev_side(&repo, second)?)
        }
        _ => unreachable!("clap limits diff to two revisions"),
    };

    let renames = RenameOptions {
        enabled: !no_renames,
        threshold: rename_threshold.unwrap_or(50),
    };
    let mut changes = diff_trees_with(&left.map, &right.map, renames.enabled);
    detect_renames(&left, &right, &mut changes, renames)?;
    if let Some(filter) = &filter {
        changes.retain(|change| filter.contains(change.kind.letter()));
    }
    if !specs.is_empty() {
        changes.retain(|change| change_matches(change, &specs));
    }
    if changes.is_empty() {
        return Ok(ExitCode::Success);
    }

    if name_only {
        for change in &changes {
            println!("{}", path::display(&change.path));
        }
        return Ok(if exit_code {
            ExitCode::Diff
        } else {
            ExitCode::Success
        });
    }
    if name_status {
        for change in &changes {
            if change.kind == FileChangeKind::Renamed {
                let old = change.old_path.as_deref().unwrap_or(&change.path);
                println!(
                    "R{}\t{}\t{}",
                    change.similarity.unwrap_or(100),
                    path::display(old),
                    path::display(&change.path)
                );
            } else {
                println!("{}\t{}", change.kind.letter(), path::display(&change.path));
            }
        }
        return Ok(if exit_code {
            ExitCode::Diff
        } else {
            ExitCode::Success
        });
    }

    let whitespace = WhitespaceOptions {
        ignore_all: ignore_all_space,
        ignore_change: ignore_space_change,
        ignore_eol: ignore_space_at_eol,
    };
    let options = render_options(
        &repo,
        unified,
        algorithm.as_deref().map(DiffAlgorithm::from_config),
        whitespace,
    );

    let mut rendered = Vec::new();
    for change in &changes {
        let output = render_diff(change, &left, &right, &options)?;
        if !output.text.is_empty() {
            rendered.push((change, output));
        }
    }

    if stat {
        let mut insertions = 0usize;
        let mut deletions = 0usize;
        for (change, output) in &rendered {
            if change.kind == FileChangeKind::Renamed {
                let old = change.old_path.as_deref().unwrap_or(&change.path);
                let stats = output.stats.unwrap_or_default();
                insertions += stats.added;
                deletions += stats.removed;
                if stats.added + stats.removed == 0 {
                    println!(
                        " {} => {} | 0",
                        path::display(old),
                        path::display(&change.path)
                    );
                } else {
                    println!(
                        " {} => {} | {} insertions(+), {} deletions(-)",
                        path::display(old),
                        path::display(&change.path),
                        stats.added,
                        stats.removed
                    );
                }
                continue;
            }
            match output.stats {
                Some(stats) => {
                    insertions += stats.added;
                    deletions += stats.removed;
                    println!(
                        " {} | {} insertions(+), {} deletions(-)",
                        path::display(&change.path),
                        stats.added,
                        stats.removed
                    );
                }
                None => println!(" {} | Bin", path::display(&change.path)),
            }
        }
        if !rendered.is_empty() {
            println!(
                "{} {} changed, {} {}(+), {} {}(-)",
                rendered.len(),
                plural(rendered.len(), "file", "files"),
                insertions,
                plural(insertions, "insertion", "insertions"),
                deletions,
                plural(deletions, "deletion", "deletions")
            );
        }
    } else {
        for (_, output) in &rendered {
            print!("{}", crate::output::colorize_patch(&output.text));
            if !output.text.ends_with('\n') {
                println!();
            }
            println!();
        }
    }

    if exit_code && !rendered.is_empty() {
        Ok(ExitCode::Diff)
    } else {
        Ok(ExitCode::Success)
    }
}

fn plural<'a>(count: usize, singular: &'a str, plural: &'a str) -> &'a str {
    if count == 1 { singular } else { plural }
}

fn change_matches(change: &FileChange, specs: &[Vec<u8>]) -> bool {
    specs.iter().any(|spec| {
        path::is_within(spec, &change.path)
            || change
                .old_path
                .as_ref()
                .is_some_and(|old| path::is_within(spec, old))
    })
}

fn render_options(
    repo: &Repository,
    unified: Option<u32>,
    algorithm: Option<DiffAlgorithm>,
    whitespace: WhitespaceOptions,
) -> RenderOptions {
    RenderOptions {
        context: unified
            .map(|value| value as usize)
            .unwrap_or(repo.config.diff.context as usize),
        algorithm: algorithm
            .unwrap_or_else(|| DiffAlgorithm::from_config(&repo.config.diff.algorithm)),
        max_inline_bytes: repo.config.diff.max_inline_bytes as usize,
        whitespace,
    }
}

fn cmd_log(
    start: &std::path::Path,
    oneline: bool,
    limit: Option<usize>,
    stat: bool,
    reverse: bool,
    paths: &[PathBuf],
) -> Result<ExitCode> {
    let repo = Repository::open(start)?;
    let specs = resolve_specs(&repo, start, paths)?;
    let entries = log_entries(
        &repo,
        &LogOptions {
            limit,
            reverse,
            stat,
            paths: specs,
        },
    )?;

    for (index, entry) in entries.iter().enumerate() {
        if index > 0 && !oneline {
            println!();
        }
        if oneline {
            let subject = entry.commit.message.lines().next().unwrap_or("");
            println!("{} {subject}", entry.oid.short());
        } else {
            println!("commit {}", entry.oid);
            println!(
                "Author: {} <{}>",
                entry.commit.author.name, entry.commit.author.email
            );
            println!(
                "Date:   {}",
                time::format_timestamp(
                    entry.commit.author.timestamp,
                    entry.commit.author.offset_minutes
                )
            );
            println!();
            for line in entry.commit.message.lines() {
                println!("    {line}");
            }
        }
        if stat {
            let (left, right) = crate::diff::commit_sides(&repo, &entry.commit)?;
            let mut changes = diff_trees(&left.map, &right.map);
            detect_renames(&left, &right, &mut changes, RenameOptions::default())?;
            if !changes.is_empty() {
                println!();
            }
            let options = render_options(&repo, None, None, WhitespaceOptions::default());
            let mut insertions = 0usize;
            let mut deletions = 0usize;
            for change in &changes {
                let rendered = render_diff(change, &left, &right, &options)?;
                if change.kind == FileChangeKind::Renamed {
                    let old = change.old_path.as_deref().unwrap_or(&change.path);
                    let stats = rendered.stats.unwrap_or_default();
                    insertions += stats.added;
                    deletions += stats.removed;
                    if stats.added + stats.removed == 0 {
                        println!(
                            "  {} => {} | 0",
                            path::display(old),
                            path::display(&change.path)
                        );
                    } else {
                        println!(
                            "  {} => {} | {} +, {} -",
                            path::display(old),
                            path::display(&change.path),
                            stats.added,
                            stats.removed
                        );
                    }
                    continue;
                }
                match rendered.stats {
                    Some(stats) => {
                        insertions += stats.added;
                        deletions += stats.removed;
                        println!(
                            "  {} {} | {} +, {} -",
                            change.kind.letter(),
                            path::display(&change.path),
                            stats.added,
                            stats.removed
                        );
                    }
                    None => println!(
                        "  {} {} | Bin",
                        change.kind.letter(),
                        path::display(&change.path)
                    ),
                }
            }
            if !changes.is_empty() {
                println!(
                    "  {} {} changed, {} {}(+), {} {}(-)",
                    changes.len(),
                    plural(changes.len(), "file", "files"),
                    insertions,
                    plural(insertions, "insertion", "insertions"),
                    deletions,
                    plural(deletions, "deletion", "deletions")
                );
            }
        }
    }
    Ok(ExitCode::Success)
}

fn cmd_show(start: &std::path::Path, spec: &str) -> Result<ExitCode> {
    let repo = Repository::open(start)?;
    match show(&repo, spec)? {
        ShowOutput::Blob(bytes) => {
            use std::io::Write;
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(&bytes)?;
            stdout.flush()?;
        }
        ShowOutput::Commit(view) => {
            println!("commit {}", view.oid);
            println!(
                "Author: {} <{}>",
                view.commit.author.name, view.commit.author.email
            );
            println!(
                "Date:   {}",
                time::format_timestamp(
                    view.commit.author.timestamp,
                    view.commit.author.offset_minutes
                )
            );
            println!();
            for line in view.commit.message.lines() {
                println!("    {line}");
            }
            println!();
            let (left, right) = crate::diff::commit_sides(&repo, &view.commit)?;
            let mut changes = diff_trees(&left.map, &right.map);
            detect_renames(&left, &right, &mut changes, RenameOptions::default())?;
            if changes.is_empty() {
                println!("(no changes)");
            } else {
                let options = render_options(&repo, None, None, WhitespaceOptions::default());
                for change in &changes {
                    let rendered = render_diff(change, &left, &right, &options)?;
                    print!("{}", crate::output::colorize_patch(&rendered.text));
                    if !rendered.text.ends_with('\n') {
                        println!();
                    }
                    println!();
                }
            }
        }
    }
    Ok(ExitCode::Success)
}

fn cmd_commit(
    start: &std::path::Path,
    message: String,
    allow_empty: bool,
    allow_large_files: bool,
    paths: &[PathBuf],
) -> Result<ExitCode> {
    let repo = Repository::open(start)?;
    let specs = resolve_specs(&repo, start, paths)?;
    let outcome = create_commit(
        &repo,
        &CommitOptions {
            message: message.clone(),
            allow_empty,
            allow_large_files,
            paths: specs,
        },
    )?;

    let subject = message.lines().next().unwrap_or("");
    println!("[main {}] {subject}", outcome.oid.short());
    if outcome.changed > 0 {
        println!(" {} file(s) changed", outcome.changed);
    }
    if outcome.remaining > 0 {
        println!(" {} change(s) remain uncommitted", outcome.remaining);
    }
    Ok(ExitCode::Success)
}

fn resolve_specs(
    repo: &Repository,
    start: &std::path::Path,
    paths: &[PathBuf],
) -> Result<Vec<Vec<u8>>> {
    let prefix = start
        .strip_prefix(&repo.root)
        .unwrap_or_else(|_| std::path::Path::new(""));
    let mut specs = Vec::with_capacity(paths.len());
    for input in paths {
        let normalized = path::normalize(input)?;
        let full = prefix.join(normalized);
        specs.push(path::path_to_bytes(&full));
    }
    Ok(specs)
}

fn cmd_status(start: &std::path::Path, include_ignored: bool) -> Result<ExitCode> {
    let repo = Repository::open(start)?;
    let report = compute_status(&repo, include_ignored)?;

    if report.unborn {
        println!("no commits yet");
    }
    if report.total_changes() == 0 {
        println!("working tree clean");
    } else {
        println!("Changes not committed:");
        for entry in &report.added {
            println!(
                "  {} {}",
                crate::output::paint(crate::output::green(), "new file:"),
                path::display(entry)
            );
        }
        for entry in &report.modified {
            println!(
                "  {} {}",
                crate::output::paint(crate::output::yellow(), "modified:"),
                path::display(entry)
            );
        }
        for entry in &report.deleted {
            println!(
                "  {} {}",
                crate::output::paint(crate::output::red(), "deleted: "),
                path::display(entry)
            );
        }
        println!();
        println!("{} change(s)", report.total_changes());
    }

    if include_ignored && !report.ignored.is_empty() {
        println!();
        println!("Ignored files:");
        for entry in &report.ignored {
            println!("  {}", path::display(entry));
        }
    }

    for warning in &report.warnings {
        eprintln!("warning: {warning}");
    }
    Ok(ExitCode::Success)
}

fn cmd_ignore(start: &std::path::Path, command: IgnoreCommand) -> Result<ExitCode> {
    let repo = Repository::open(start)?;
    match command {
        IgnoreCommand::List => {
            for file in ignore_rules::files(&repo.root)? {
                println!("{}:", path::display(&path::path_to_bytes(&file)));
                for rule in ignore_rules::read_rules(&repo.root.join(&file))? {
                    println!("  {rule}");
                }
            }
        }
        IgnoreCommand::Check { path: input } => {
            let normalized = path::normalize(&input)?;
            let relative = path::path_to_bytes(&normalized);
            let verdict = ignore_rules::check(&repo.root, &relative)?;
            if verdict.ignored {
                println!("ignored");
            } else {
                println!("not ignored");
            }
            if let Some(pattern) = &verdict.pattern {
                println!("  pattern: {pattern}");
            }
            if let Some(source) = &verdict.source {
                println!("  source: {}", source.display());
            }
            if verdict.builtin {
                println!("  source: built-in");
            }
        }
    }
    Ok(ExitCode::Success)
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
