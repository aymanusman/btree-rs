# btree-rs

A generic, disk-backed **B+ tree** written in pure Rust — no unsafe code, no
external C libraries.

[![CI](https://github.com/aymanusman/btree-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/aymanusman/btree-rs/actions)

## Features

- **Fully generic** — `BTree<K, V>` works with any key type that implements
  `Ord + Serialize + DeserializeOwned` and any serializable value.
- **Disk persistence** — nodes are stored as fixed-size 4 KB pages in a single
  file. Data survives process restarts.
- **Linked leaf chain** — leaf nodes are singly linked, so full range scans
  never touch internal nodes.
- **Lazy cursor** — `range()` returns a `Cursor` that loads one leaf at a time,
  keeping memory usage constant even over very large datasets.
- **Configurable order** — the minimum degree `t` is set at open time (default
  suggestion: `t = 50`, giving up to 99 keys per node in a 4 KB page).

## Quick start

```rust
use btree::BTree;
use std::ops::Bound;

// Open (or create) a tree backed by a file.
let mut tree: BTree<String, u64> = BTree::open("data.db", 50)?;

// Insert and retrieve.
tree.insert("alice".to_string(), 1)?;
tree.insert("bob".to_string(), 2)?;
tree.insert("carol".to_string(), 3)?;

assert_eq!(tree.get(&"alice".to_string())?, 1);

// Full scan — returns all pairs in sorted key order.
let all = tree.scan_all()?;
// → [("alice", 1), ("bob", 2), ("carol", 3)]

// Range scan with a Cursor.
let mut cursor = tree.range(
    Bound::Included("alice".to_string()),
    Bound::Excluded("carol".to_string()),
)?;
// Advance by calling cursor.next(&mut pager) — see examples/ for full usage.

// Delete.
tree.delete(&"bob".to_string())?;
```

## CLI

```bash
cargo build --release
./target/release/btree-cli data.db set alice 1
./target/release/btree-cli data.db get alice   # prints: 1
./target/release/btree-cli data.db del alice
./target/release/btree-cli data.db scan        # prints all key => value pairs
```

## Running the tests

```bash
cargo test
```

The test suite covers:

- Basic insert, get, overwrite, delete
- `KeyNotFound` and `InvalidOrder` error cases  
- Sorted-order invariant after 100 reverse-order inserts
- Sorted-order invariant after 200 pseudo-random inserts (verified against
  `std::collections::BTreeMap`)
- 500-key insert with no data loss
- Full scan correctness
- Bounded range scan correctness
- Disk persistence across process restarts
- Generic key types (`u32`, `String`)

## Design

See [ARCHITECTURE.md](ARCHITECTURE.md) for a full breakdown of module
responsibilities, the page file format, the proactive-split insert strategy,
and known limitations.

## Project context

This crate is **Project 1** in a three-part series:

1. **btree-rs** (this repo) — generic B+ tree storage engine
2. **kvstore-rs** — concurrent key-value store using `btree-rs` as its disk
   engine, with a WAL, LRU cache, and async TCP server
3. **raft-kv-rs** — distributed key-value store using Raft consensus, with
   `kvstore-rs` as the replicated state machine

## License

MIT
