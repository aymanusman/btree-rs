//! Core B-tree logic: insert, search, delete, and range scan.
//!
//! All nodes live on disk via the `Pager`. We load a node, mutate it in
//! memory, write it back, and flush at the end of each public operation.
//!
//! Split strategy: proactive top-down splits (Knuth's approach). Before
//! descending into a full child we split it immediately, so we never need
//! to walk back up the tree. This keeps the recursion simple and clean.

use crate::cursor::Cursor;
use crate::error::{BTreeError, Result};
use crate::node::{BTreeNode, InternalNode, LeafNode};
use crate::pager::{Pager, NO_PAGE};
use serde::{de::DeserializeOwned, Serialize};
use std::ops::Bound;
use std::path::Path;

/// A disk-backed B+ tree.
///
/// # Type parameters
/// - `K`: key type. Must be `Ord`, `Serialize`, `DeserializeOwned`, `Clone`.
/// - `V`: value type. Must be `Serialize`, `DeserializeOwned`, `Clone`.
///
/// # Order
/// `t` is the minimum degree. Every non-root node has between `t-1` and `2t-1`
/// keys. The default is `t = 50` (up to 99 keys per node, fits in a 4 KB page
/// for most key/value types).
pub struct BTree<K, V> {
    pager: Pager,
    /// Minimum degree.
    t: usize,
    _phantom: std::marker::PhantomData<(K, V)>,
}

impl<K, V> BTree<K, V>
where
    K: Ord + Clone + Serialize + DeserializeOwned,
    V: Clone + Serialize + DeserializeOwned,
{
    /// Open (or create) a B-tree backed by the file at `path`.
    pub fn open(path: impl AsRef<Path>, t: usize) -> Result<Self> {
        if t < 2 {
            return Err(BTreeError::InvalidOrder { got: t });
        }
        let mut pager = Pager::open(path)?;

        // If this is a brand-new file, create the root leaf.
        if pager.root_page_id == NO_PAGE {
            let root_id = pager.alloc_page()?;
            let root = BTreeNode::<K, V>::new_leaf();
            pager.write_node(root_id, &root)?;
            pager.root_page_id = root_id;
            pager.flush()?;
        }

        Ok(BTree {
            pager,
            t,
            _phantom: std::marker::PhantomData,
        })
    }

    // ── Public API ────────────────────────────────────────────────────────────

    /// Look up `key`. Returns `Ok(value)` or `Err(KeyNotFound)`.
    pub fn get(&mut self, key: &K) -> Result<V> {
        let root_id = self.pager.root_page_id;
        self.search(root_id, key)
    }

    /// Insert or overwrite `key` → `value`.
    pub fn insert(&mut self, key: K, value: V) -> Result<()> {
        let root_id = self.pager.root_page_id;
        let root: BTreeNode<K, V> = self.pager.read_node(root_id)?;

        if root.is_full(self.t) {
            // Root is full — grow the tree by one level.
            let new_root_id = self.pager.alloc_page()?;
            let new_root = BTreeNode::<K, V>::Internal(InternalNode {
                keys: Vec::new(),
                children: vec![root_id],
            });
            self.pager.write_node(new_root_id, &new_root)?;
            self.pager.root_page_id = new_root_id;
            self.split_child(new_root_id, 0, root_id)?;
            self.insert_non_full(new_root_id, key, value)?;
        } else {
            self.insert_non_full(root_id, key, value)?;
        }

        self.pager.flush()?;
        Ok(())
    }

    /// Delete `key`. Returns `Err(KeyNotFound)` if absent.
    pub fn delete(&mut self, key: &K) -> Result<()> {
        let root_id = self.pager.root_page_id;
        self.delete_from(root_id, key)?;

        // If root is now an empty internal node, shrink the tree.
        let root: BTreeNode<K, V> = self.pager.read_node(root_id)?;
        if let BTreeNode::Internal(ref n) = root {
            if n.keys.is_empty() && !n.children.is_empty() {
                self.pager.root_page_id = n.children[0];
            }
        }

        self.pager.flush()?;
        Ok(())
    }

    /// Return a `Cursor` for the half-open range `[start, end)`.
    /// Pass `Bound::Unbounded` for open-ended ranges.
    pub fn range(&mut self, start: Bound<K>, end: Bound<K>) -> Result<Cursor<K, V>> {
        let root_id = self.pager.root_page_id;
        let (leaf, start_pos) = self.find_leaf_and_pos(root_id, &start)?;
        Ok(Cursor::new(leaf, start_pos, end))
    }

    /// Convenience: collect all key-value pairs in sorted order.
    pub fn scan_all(&mut self) -> Result<Vec<(K, V)>> {
        let mut cursor = self.range(Bound::Unbounded, Bound::Unbounded)?;
        cursor.collect_all(&mut self.pager)
    }

    // ── Search ────────────────────────────────────────────────────────────────

    fn search(&mut self, page_id: u64, key: &K) -> Result<V> {
        let node: BTreeNode<K, V> = self.pager.read_node(page_id)?;
        match node {
            BTreeNode::Leaf(leaf) => match leaf.keys.binary_search(key) {
                Ok(i) => Ok(leaf.values[i].clone()),
                Err(_) => Err(BTreeError::KeyNotFound),
            },
            BTreeNode::Internal(internal) => {
                let child_idx = match internal.keys.binary_search(key) {
                    Ok(i) => i + 1,  // go right of equal separator
                    Err(i) => i,
                };
                self.search(internal.children[child_idx], key)
            }
        }
    }

    // ── Insert helpers ────────────────────────────────────────────────────────

    fn insert_non_full(&mut self, page_id: u64, key: K, value: V) -> Result<()> {
        let node: BTreeNode<K, V> = self.pager.read_node(page_id)?;

        match node {
            BTreeNode::Leaf(mut leaf) => {
                match leaf.keys.binary_search(&key) {
                    Ok(i) => {
                        // Key exists — overwrite value.
                        leaf.values[i] = value;
                    }
                    Err(i) => {
                        leaf.keys.insert(i, key);
                        leaf.values.insert(i, value);
                    }
                }
                self.pager.write_node(page_id, &BTreeNode::Leaf(leaf))?;
            }
            BTreeNode::Internal(mut internal) => {
                let child_idx = match internal.keys.binary_search(&key) {
                    Ok(i) => i + 1,
                    Err(i) => i,
                };
                let child_id = internal.children[child_idx];
                let child: BTreeNode<K, V> = self.pager.read_node(child_id)?;

                if child.is_full(self.t) {
                    self.split_child(page_id, child_idx, child_id)?;
                    // Re-read parent after split (separator key was inserted).
                    let updated: BTreeNode<K, V> = self.pager.read_node(page_id)?;
                    if let BTreeNode::Internal(updated_internal) = updated {
                        internal = updated_internal;
                    }
                    // Decide which of the two post-split children to descend into.
                    let new_child_idx = match internal.keys.binary_search(&key) {
                        Ok(i) => i + 1,
                        Err(i) => i,
                    };
                    self.insert_non_full(internal.children[new_child_idx], key, value)?;
                } else {
                    self.insert_non_full(child_id, key, value)?;
                }
            }
        }
        Ok(())
    }

    /// Split `child_id` (the `child_idx`-th child of `parent_id`) into two nodes.
    /// The median key is promoted into the parent.
    fn split_child(&mut self, parent_id: u64, child_idx: usize, child_id: u64) -> Result<()> {
        let child: BTreeNode<K, V> = self.pager.read_node(child_id)?;
        let t = self.t;

        match child {
            BTreeNode::Leaf(mut left_leaf) => {
                // For B+ trees: the median key is *copied* up (not moved),
                // so both leaves retain it. Split at index t-1.
                let right_keys = left_leaf.keys.split_off(t - 1);
                let right_values = left_leaf.values.split_off(t - 1);
                let old_next = left_leaf.next_leaf;

                let right_id = self.pager.alloc_page()?;
                let right_leaf = LeafNode {
                    keys: right_keys,
                    values: right_values,
                    next_leaf: old_next,
                };
                left_leaf.next_leaf = right_id;

                // Promoted separator = smallest key in right leaf.
                let separator = right_leaf.keys[0].clone();

                self.pager.write_node(child_id, &BTreeNode::Leaf(left_leaf))?;
                self.pager.write_node(right_id, &BTreeNode::Leaf(right_leaf))?;

                // Insert separator and new child pointer into parent.
                let parent: BTreeNode<K, V> = self.pager.read_node(parent_id)?;
                if let BTreeNode::Internal(mut p) = parent {
                    p.keys.insert(child_idx, separator);
                    p.children.insert(child_idx + 1, right_id);
                    self.pager.write_node(parent_id, &BTreeNode::<K, V>::Internal(p))?;
                }
            }

            BTreeNode::Internal(mut left_internal) => {
                // For internal nodes: the median key is *moved* up.
                let right_keys = left_internal.keys.split_off(t); // t..2t-1
                let median = left_internal.keys.pop().unwrap();    // index t-1
                let right_children = left_internal.children.split_off(t);

                let right_id = self.pager.alloc_page()?;
                let right_internal = InternalNode {
                    keys: right_keys,
                    children: right_children,
                };

                self.pager
                    .write_node(child_id, &BTreeNode::<K, V>::Internal(left_internal))?;
                self.pager
                    .write_node(right_id, &BTreeNode::<K, V>::Internal(right_internal))?;

                let parent: BTreeNode<K, V> = self.pager.read_node(parent_id)?;
                if let BTreeNode::Internal(mut p) = parent {
                    p.keys.insert(child_idx, median);
                    p.children.insert(child_idx + 1, right_id);
                    self.pager.write_node(parent_id, &BTreeNode::<K, V>::Internal(p))?;
                }
            }
        }
        Ok(())
    }

    // ── Delete helpers ────────────────────────────────────────────────────────

    fn delete_from(&mut self, page_id: u64, key: &K) -> Result<()> {
        let node: BTreeNode<K, V> = self.pager.read_node(page_id)?;

        match node {
            BTreeNode::Leaf(mut leaf) => {
                match leaf.keys.binary_search(key) {
                    Ok(i) => {
                        leaf.keys.remove(i);
                        leaf.values.remove(i);
                        self.pager.write_node(page_id, &BTreeNode::Leaf(leaf))?;
                        Ok(())
                    }
                    Err(_) => Err(BTreeError::KeyNotFound),
                }
            }

            BTreeNode::Internal(mut internal) => {
                let child_idx = match internal.keys.binary_search(key) {
                    Ok(i) => i + 1,
                    Err(i) => i,
                };
                let child_id = internal.children[child_idx];
                let child: BTreeNode<K, V> = self.pager.read_node(child_id)?;

                // Ensure child has at least t keys before descending so we can
                // borrow or merge without walking back up.
                if child.num_keys() < self.t {
                    self.fix_child(&mut internal, page_id, child_idx)?;
                    // Re-read after restructuring and re-derive child_idx.
                    let updated: BTreeNode<K, V> = self.pager.read_node(page_id)?;
                    if let BTreeNode::Internal(updated_internal) = updated {
                        let new_idx = match updated_internal.keys.binary_search(key) {
                            Ok(i) => i + 1,
                            Err(i) => i,
                        };
                        return self.delete_from(updated_internal.children[new_idx], key);
                    }
                }

                self.delete_from(child_id, key)?;

                // Update separator key in parent if we deleted from the left boundary.
                self.pager.write_node(page_id, &BTreeNode::<K, V>::Internal(internal))?;
                Ok(())
            }
        }
    }

    /// Ensure `internal.children[child_idx]` has at least `t` keys by borrowing
    /// from a sibling or merging with one.
    fn fix_child(
        &mut self,
        internal: &mut InternalNode<K>,
        parent_page_id: u64,
        child_idx: usize,
    ) -> Result<()> {
        let left_sibling = if child_idx > 0 {
            Some(internal.children[child_idx - 1])
        } else {
            None
        };
        let right_sibling = if child_idx < internal.children.len() - 1 {
            Some(internal.children[child_idx + 1])
        } else {
            None
        };

        let child_id = internal.children[child_idx];

        // Try borrow from left sibling.
        if let Some(left_id) = left_sibling {
            let left: BTreeNode<K, V> = self.pager.read_node(left_id)?;
            if left.num_keys() >= self.t {
                self.borrow_from_left(internal, parent_page_id, child_idx, left_id, child_id)?;
                return Ok(());
            }
        }

        // Try borrow from right sibling.
        if let Some(right_id) = right_sibling {
            let right: BTreeNode<K, V> = self.pager.read_node(right_id)?;
            if right.num_keys() >= self.t {
                self.borrow_from_right(internal, parent_page_id, child_idx, child_id, right_id)?;
                return Ok(());
            }
        }

        // Neither sibling has spare keys — merge.
        if left_sibling.is_some() {
            let left_id = internal.children[child_idx - 1];
            self.merge(internal, parent_page_id, child_idx - 1, left_id, child_id)?;
        } else if let Some(right_id) = right_sibling {
            self.merge(internal, parent_page_id, child_idx, child_id, right_id)?;
        }

        Ok(())
    }

    fn borrow_from_left(
        &mut self,
        internal: &mut InternalNode<K>,
        parent_page_id: u64,
        child_idx: usize,
        left_id: u64,
        child_id: u64,
    ) -> Result<()> {
        let left_node: BTreeNode<K, V> = self.pager.read_node(left_id)?;
        let child_node: BTreeNode<K, V> = self.pager.read_node(child_id)?;

        match (left_node, child_node) {
            (BTreeNode::Leaf(mut left), BTreeNode::Leaf(mut child)) => {
                let borrow_key = left.keys.pop().unwrap();
                let borrow_val = left.values.pop().unwrap();
                child.keys.insert(0, borrow_key.clone());
                child.values.insert(0, borrow_val);
                // Update separator in parent.
                internal.keys[child_idx - 1] = borrow_key;
                self.pager.write_node(left_id, &BTreeNode::Leaf(left))?;
                self.pager.write_node(child_id, &BTreeNode::Leaf(child))?;
                self.pager.write_node(parent_page_id, &BTreeNode::<K, V>::Internal(internal.clone()))?;
            }
            (BTreeNode::Internal(mut left), BTreeNode::Internal(mut child)) => {
                let sep = internal.keys[child_idx - 1].clone();
                let borrow_key = left.keys.pop().unwrap();
                let borrow_child = left.children.pop().unwrap();
                child.keys.insert(0, sep);
                child.children.insert(0, borrow_child);
                internal.keys[child_idx - 1] = borrow_key;
                self.pager.write_node(left_id, &BTreeNode::<K, V>::Internal(left))?;
                self.pager.write_node(child_id, &BTreeNode::<K, V>::Internal(child))?;
                self.pager.write_node(parent_page_id, &BTreeNode::<K, V>::Internal(internal.clone()))?;
            }
            _ => {}
        }
        Ok(())
    }

    fn borrow_from_right(
        &mut self,
        internal: &mut InternalNode<K>,
        parent_page_id: u64,
        child_idx: usize,
        child_id: u64,
        right_id: u64,
    ) -> Result<()> {
        let child_node: BTreeNode<K, V> = self.pager.read_node(child_id)?;
        let right_node: BTreeNode<K, V> = self.pager.read_node(right_id)?;

        match (child_node, right_node) {
            (BTreeNode::Leaf(mut child), BTreeNode::Leaf(mut right)) => {
                let borrow_key = right.keys.remove(0);
                let borrow_val = right.values.remove(0);
                child.keys.push(borrow_key);
                child.values.push(borrow_val);
                // New separator = new first key of right leaf.
                internal.keys[child_idx] = right.keys[0].clone();
                self.pager.write_node(child_id, &BTreeNode::Leaf(child))?;
                self.pager.write_node(right_id, &BTreeNode::Leaf(right))?;
                self.pager.write_node(parent_page_id, &BTreeNode::<K, V>::Internal(internal.clone()))?;
            }
            (BTreeNode::Internal(mut child), BTreeNode::Internal(mut right)) => {
                let sep = internal.keys[child_idx].clone();
                let borrow_key = right.keys.remove(0);
                let borrow_child = right.children.remove(0);
                child.keys.push(sep);
                child.children.push(borrow_child);
                internal.keys[child_idx] = borrow_key;
                self.pager.write_node(child_id, &BTreeNode::<K, V>::Internal(child))?;
                self.pager.write_node(right_id, &BTreeNode::<K, V>::Internal(right))?;
                self.pager.write_node(parent_page_id, &BTreeNode::<K, V>::Internal(internal.clone()))?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Merge `right_id` into `left_id`, pulling down the separator from the parent.
    fn merge(
        &mut self,
        internal: &mut InternalNode<K>,
        parent_page_id: u64,
        sep_idx: usize,
        left_id: u64,
        right_id: u64,
    ) -> Result<()> {
        let left_node: BTreeNode<K, V> = self.pager.read_node(left_id)?;
        let right_node: BTreeNode<K, V> = self.pager.read_node(right_id)?;

        match (left_node, right_node) {
            (BTreeNode::Leaf(mut left), BTreeNode::Leaf(right)) => {
                left.keys.extend(right.keys);
                left.values.extend(right.values);
                left.next_leaf = right.next_leaf;
                internal.keys.remove(sep_idx);
                internal.children.remove(sep_idx + 1);
                self.pager.write_node(left_id, &BTreeNode::Leaf(left))?;
                self.pager.write_node(parent_page_id, &BTreeNode::<K, V>::Internal(internal.clone()))?;
            }
            (BTreeNode::Internal(mut left), BTreeNode::Internal(right)) => {
                let sep = internal.keys.remove(sep_idx);
                internal.children.remove(sep_idx + 1);
                left.keys.push(sep);
                left.keys.extend(right.keys);
                left.children.extend(right.children);
                self.pager.write_node(left_id, &BTreeNode::<K, V>::Internal(left))?;
                self.pager.write_node(parent_page_id, &BTreeNode::<K, V>::Internal(internal.clone()))?;
            }
            _ => {}
        }
        Ok(())
    }

    // ── Range scan helper ─────────────────────────────────────────────────────

    /// Walk down to the leaf that should contain the start of the range,
    /// returning that leaf and the starting position within it.
    fn find_leaf_and_pos(
        &mut self,
        page_id: u64,
        start: &Bound<K>,
    ) -> Result<(LeafNode<K, V>, usize)> {
        let node: BTreeNode<K, V> = self.pager.read_node(page_id)?;
        match node {
            BTreeNode::Leaf(leaf) => {
                let pos = match start {
                    Bound::Unbounded => 0,
                    Bound::Included(lo) => match leaf.keys.binary_search(lo) {
                        Ok(i) | Err(i) => i,
                    },
                    Bound::Excluded(lo) => match leaf.keys.binary_search(lo) {
                        Ok(i) => i + 1,
                        Err(i) => i,
                    },
                };
                Ok((leaf, pos))
            }
            BTreeNode::Internal(internal) => {
                let child_idx = match start {
                    Bound::Unbounded => 0,
                    Bound::Included(lo) | Bound::Excluded(lo) => {
                        match internal.keys.binary_search(lo) {
                            Ok(i) => i + 1,
                            Err(i) => i,
                        }
                    }
                };
                self.find_leaf_and_pos(internal.children[child_idx], start)
            }
        }
    }
}