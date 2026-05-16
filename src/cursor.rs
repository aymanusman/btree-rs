//! Cursor: a forward iterator over the leaf chain for range scans.
//!
//! Leaves are singly-linked via `LeafNode::next_leaf`. A cursor holds the
//! current leaf in memory and a position within it, advancing through the
//! chain without loading the whole tree into memory.

use crate::error::Result;
use crate::node::{BTreeNode, LeafNode};
use crate::pager::{Pager, NO_PAGE};
use serde::{de::DeserializeOwned, Serialize};
use std::ops::Bound;

pub struct Cursor<K, V> {
    /// Current leaf node being iterated.
    current_leaf: LeafNode<K, V>,
    /// Position within `current_leaf.keys`.
    pos: usize,
    /// Upper bound for the range scan (exclusive or included).
    end: Bound<K>,
}

impl<K, V> Cursor<K, V>
where
    K: Ord + Clone + Serialize + DeserializeOwned,
    V: Clone + Serialize + DeserializeOwned,
{
    /// Create a cursor starting at `start_pos` within `leaf`.
    /// `end` is the upper bound of the range (can be Unbounded).
    pub fn new(leaf: LeafNode<K, V>, start_pos: usize, end: Bound<K>) -> Self {
        Cursor {
            current_leaf: leaf,
            pos: start_pos,
            end,
        }
    }

    /// Advance to the next key-value pair, crossing leaf boundaries if needed.
    /// Returns `None` when the range is exhausted or `Err` on I/O failure.
    pub fn next(&mut self, pager: &mut Pager) -> Result<Option<(K, V)>> {
        loop {
            if self.pos < self.current_leaf.keys.len() {
                let key = &self.current_leaf.keys[self.pos];

                // Check upper bound.
                let past_end = match &self.end {
                    Bound::Unbounded => false,
                    Bound::Included(hi) => key > hi,
                    Bound::Excluded(hi) => key >= hi,
                };
                if past_end {
                    return Ok(None);
                }

                let k = self.current_leaf.keys[self.pos].clone();
                let v = self.current_leaf.values[self.pos].clone();
                self.pos += 1;
                return Ok(Some((k, v)));
            }

            // Current leaf exhausted — try next leaf in chain.
            let next_id = self.current_leaf.next_leaf;
            if next_id == NO_PAGE {
                return Ok(None);
            }

            let node: BTreeNode<K, V> = pager.read_node(next_id)?;
            match node {
                BTreeNode::Leaf(leaf) => {
                    self.current_leaf = leaf;
                    self.pos = 0;
                }
                BTreeNode::Internal(_) => {
                    // Leaf chain should never point to an internal node.
                    return Ok(None);
                }
            }
        }
    }

    /// Collect all remaining items into a Vec (convenience helper for tests).
    pub fn collect_all(&mut self, pager: &mut Pager) -> Result<Vec<(K, V)>> {
        let mut result = Vec::new();
        while let Some(pair) = self.next(pager)? {
            result.push(pair);
        }
        Ok(result)
    }
}