//! # btree-rs
//!
//! A generic, disk-backed B+ tree in pure Rust.
//!
//! ## Features
//! - **Generic keys and values** — any `K: Ord + Serialize + DeserializeOwned`
//!   and `V: Serialize + DeserializeOwned`.
//! - **Disk persistence** — nodes are stored as fixed-size 4 KB pages in a
//!   single file via the `Pager`.
//! - **Linked leaf chain** — leaves are singly linked so range scans never
//!   touch internal nodes.
//! - **Cursor iterator** — lazy forward iteration with `Bound`-based start/end.
//!
//! ## Quick start
//! ```rust,no_run
//! use btree::{BTree, BTreeError};
//! use std::ops::Bound;
//!
//! let mut tree: BTree<String, u64> = BTree::open("/tmp/my.db", 50).unwrap();
//! tree.insert("alice".to_string(), 1).unwrap();
//! tree.insert("bob".to_string(), 2).unwrap();
//!
//! assert_eq!(tree.get(&"alice".to_string()).unwrap(), 1);
//! ```

pub mod cursor;
pub mod error;
pub mod node;
pub mod pager;
pub mod tree;

pub use error::{BTreeError, Result};
pub use tree::BTree;
