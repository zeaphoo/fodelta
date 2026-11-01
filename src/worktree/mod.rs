//! Working tree scanning, stat caching and status reporting.

pub mod restore;
pub mod scan;
pub mod stat_cache;
pub mod status;

pub use restore::{RestoreOptions, RestoreOutcome, restore};
pub use scan::{EntryKind, WorktreeEntry, scan, scan_all};
pub use stat_cache::{FileStat, StatCache};
pub use status::{StatusKind, StatusReport, compute_status};
