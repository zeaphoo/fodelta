//! Content addressed object database: blobs, trees and commits.

pub mod commit;
pub mod object;
pub mod store;
pub mod tree;

pub use commit::{Author, Commit};
pub use object::{ObjectId, ObjectKind};
pub use store::ObjectStore;
pub use tree::{MODE_EXEC, MODE_FILE, MODE_SYMLINK, MODE_TREE, Tree, TreeEntry};
