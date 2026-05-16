# Architecture

## Overview

`btree-rs` is a B+ tree: internal nodes hold only separator keys and child
pointers, while all key-value pairs live in leaf nodes. Leaves are singly
linked so range scans traverse only the leaf level — no internal nodes needed.

```
                    [30 | 60]          ← Internal node (root)
                   /    |    \
           [10|20]    [40|50]   [70|80]   ← Internal nodes
            /  \       /  \      /  \
          [..]  [..]  [..]  [..] [..] [..] → → → → ← leaf chain
```

## Module layout

| Module | Responsibility |
|--------|----------------|
| `error` | `BTreeError` enum via `thiserror`. Single source of truth for all failure modes. |
| `node` | `BTreeNode<K,V>` enum (`Internal` / `Leaf`). Pure data — no I/O, no algorithms. |
| `pager` | Maps page IDs to 4 KB slots in a file. Maintains a write-back cache (dirty pages) and flushes on demand. |
| `tree` | All B+ tree algorithms: search, insert (proactive top-down splits), delete (borrow/merge), and range-scan entry point. |
| `cursor` | Forward iterator over the leaf chain. Lazy — loads one leaf at a time from the pager. |

## Key design decisions

### Proactive top-down splits (insert)
When descending for an insert, we split any full child *before* recursing into
it. This means we never need to walk back up the tree to propagate a split,
keeping the recursion simple and the code easier to reason about.

### B+ tree vs B-tree
Unlike a classic B-tree (where internal nodes hold values), this is a B+ tree:
- All values live in leaf nodes only.
- Internal nodes hold separator keys purely for routing.
- The leaf chain enables O(n) sequential scans without touching internal nodes.
- Range queries are O(log n + k) where k is the number of results.

### Page format
Each page is exactly 4,096 bytes (one OS memory page). Nodes are serialized
with `bincode` (compact, deterministic, no schema overhead). We store the
serialized length followed by the payload, zero-padded to 4,096 bytes.

File layout:
```
[0..8]   next_page_id (u64, little-endian)
[8..16]  root_page_id (u64, little-endian)
[4096 * (id+1) .. 4096 * (id+2)]  page data for page `id`
```

### Deletion: borrow before merge
When a child has fewer than `t` keys and we need to delete from it, we first
try to borrow a key from a sibling. Only if neither sibling has spare keys do
we merge. This keeps tree height stable under mixed workloads.

### Write-back cache
The pager maintains an in-memory map of dirty page IDs → serialized bytes.
Writes go to the cache; `flush()` writes all dirty pages then the header.
This batches disk I/O at the end of each public operation (`insert`, `delete`).

## Trade-offs and future work

- **No WAL**: the current implementation is not crash-safe. A power failure
  during `flush()` can leave the file in an inconsistent state. Adding a
  write-ahead log (used in Project 2, `kvstore-rs`) would fix this.
- **No page recycling**: deleted pages are never reused, so the file only grows.
  A free-list in the header would allow reclaiming pages.
- **Fixed page size**: 4 KB works well for most key/value sizes but very large
  values will exceed one page. Variable-length overflow pages would be needed.
- **Single-writer**: there is no concurrency control. Wrapping the tree in an
  `Arc<RwLock<>>` (as done in Project 2) is the simplest fix.
