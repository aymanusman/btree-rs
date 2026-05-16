use serde::{Deserialize, Serialize};

/// A single node in the B-tree, stored as one page on disk.
///
/// We use an enum so the type system enforces that only leaf nodes hold values,
/// and only internal nodes hold child page pointers.
///
/// Invariants (order = t):
///   - Every non-root node has at least t-1 keys.
///   - Every node has at most 2t-1 keys.
///   - An internal node with k keys has exactly k+1 children.
///   - All leaves are at the same depth.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BTreeNode<K, V> {
    Internal(InternalNode<K>),
    Leaf(LeafNode<K, V>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalNode<K> {
    /// Separator keys: keys[i] is the smallest key in children[i+1].
    pub keys: Vec<K>,
    /// Page IDs of child nodes. Always len == keys.len() + 1.
    pub children: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeafNode<K, V> {
    pub keys: Vec<K>,
    pub values: Vec<V>,
    /// Page ID of the next leaf (for range scans). 0 means no next leaf.
    pub next_leaf: u64,
}

impl<K: Ord, V> BTreeNode<K, V> {
    pub fn new_leaf() -> Self {
        BTreeNode::Leaf(LeafNode {
            keys: Vec::new(),
            values: Vec::new(),
            next_leaf: u64::MAX,
        })
    }

    pub fn new_internal() -> Self {
        BTreeNode::Internal(InternalNode {
            keys: Vec::new(),
            children: Vec::new(),
        })
    }

    pub fn is_leaf(&self) -> bool {
        matches!(self, BTreeNode::Leaf(_))
    }

    /// Number of keys in this node.
    pub fn num_keys(&self) -> usize {
        match self {
            BTreeNode::Internal(n) => n.keys.len(),
            BTreeNode::Leaf(n) => n.keys.len(),
        }
    }

    /// True when this node has reached 2t-1 keys and must be split before insertion.
    pub fn is_full(&self, order: usize) -> bool {
        self.num_keys() >= 2 * order - 1
    }

    /// Binary search for a key in this node. Returns Ok(index) on exact match,
    /// Err(index) for the child slot to descend into.
    pub fn search_keys(&self, key: &K) -> std::result::Result<usize, usize>
    where
        K: Ord,
    {
        match self {
            BTreeNode::Internal(n) => n.keys.binary_search(key),
            BTreeNode::Leaf(n) => n.keys.binary_search(key),
        }
    }
}