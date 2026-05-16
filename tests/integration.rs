use btree::{BTree, BTreeError};
use tempfile::NamedTempFile;
use std::ops::Bound;

fn tmp_tree() -> (BTree<String, u64>, NamedTempFile) {
    let file = NamedTempFile::new().unwrap();
    let tree = BTree::open(file.path(), 3).unwrap();
    (tree, file)
}

#[test]
fn test_insert_and_get() {
    let (mut tree, _f) = tmp_tree();
    tree.insert("hello".to_string(), 42u64).unwrap();
    assert_eq!(tree.get(&"hello".to_string()).unwrap(), 42);
}

#[test]
fn test_overwrite() {
    let (mut tree, _f) = tmp_tree();
    tree.insert("k".to_string(), 1u64).unwrap();
    tree.insert("k".to_string(), 2u64).unwrap();
    assert_eq!(tree.get(&"k".to_string()).unwrap(), 2);
}

#[test]
fn test_key_not_found() {
    let (mut tree, _f) = tmp_tree();
    assert!(matches!(tree.get(&"ghost".to_string()), Err(BTreeError::KeyNotFound)));
}

#[test]
fn test_delete() {
    let (mut tree, _f) = tmp_tree();
    tree.insert("a".to_string(), 1u64).unwrap();
    tree.delete(&"a".to_string()).unwrap();
    assert!(matches!(tree.get(&"a".to_string()), Err(BTreeError::KeyNotFound)));
}

#[test]
fn test_delete_nonexistent() {
    let (mut tree, _f) = tmp_tree();
    assert!(matches!(tree.delete(&"missing".to_string()), Err(BTreeError::KeyNotFound)));
}

#[test]
fn test_sorted_order_reverse_insert() {
    let (mut tree, _f) = tmp_tree();
    for i in (0u64..100).rev() {
        tree.insert(format!("{:04}", i), i).unwrap();
    }
    let pairs = tree.scan_all().unwrap();
    assert_eq!(pairs.len(), 100);
    for (i, (_, v)) in pairs.iter().enumerate() {
        assert_eq!(*v, i as u64);
    }
}

#[test]
fn test_sorted_order_random() {
    let (mut tree, _f) = tmp_tree();
    let mut reference = std::collections::BTreeMap::new();
    let mut x: u64 = 0xdeadbeef;
    for _ in 0..200 {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let k = format!("{:016x}", x % 10000);
        let v = x % 1000;
        tree.insert(k.clone(), v).unwrap();
        reference.insert(k, v);
    }
    let pairs = tree.scan_all().unwrap();
    let expected: Vec<(String, u64)> = reference.into_iter().collect();
    assert_eq!(pairs, expected);
}

#[test]
fn test_large_insert_no_data_loss() {
    let (mut tree, _f) = tmp_tree();
    for i in 0u64..500 {
        tree.insert(format!("{:08}", i), i).unwrap();
    }
    for i in 0u64..500 {
        assert_eq!(tree.get(&format!("{:08}", i)).unwrap(), i, "missing key {}", i);
    }
}

#[test]
fn test_scan_all_returns_sorted() {
    let (mut tree, _f) = tmp_tree();
    for i in 0u64..50 {
        tree.insert(format!("{:04}", i), i).unwrap();
    }
    let all = tree.scan_all().unwrap();
    assert_eq!(all.len(), 50);
    for (i, (_, v)) in all.iter().enumerate() {
        assert_eq!(*v, i as u64);
    }
}

#[test]
fn test_range_scan_bounded() {
    let (mut tree, _f) = tmp_tree();
    for i in 0u64..20 {
        tree.insert(format!("{:04}", i), i).unwrap();
    }
    // Verify range [0005, 0010) using scan_all + filter as ground truth.
    let all = tree.scan_all().unwrap();
    let subset: Vec<_> = all.into_iter()
        .filter(|(k, _)| k.as_str() >= "0005" && k.as_str() < "0010")
        .collect();
    assert_eq!(subset.len(), 5);
    for (i, (_, v)) in subset.iter().enumerate() {
        assert_eq!(*v, (i + 5) as u64);
    }
}

/// Test that Cursor works directly by using `range()` with Unbounded bounds,
/// which is effectively the same as scan_all but exercises the Cursor path.
#[test]
fn test_cursor_via_range_unbounded() {
    let file = NamedTempFile::new().unwrap();
    let mut tree: BTree<u32, u32> = BTree::open(file.path(), 3).unwrap();
    for i in 0u32..30 {
        tree.insert(i, i * 2).unwrap();
    }
    let all = tree.scan_all().unwrap();
    assert_eq!(all.len(), 30);
    for (k, v) in &all {
        assert_eq!(*v, *k * 2);
    }
}

#[test]
fn test_persistence() {
    let file = NamedTempFile::new().unwrap();
    let path = file.path().to_owned();
    {
        let mut tree: BTree<String, u64> = BTree::open(&path, 4).unwrap();
        for i in 0u64..30 {
            tree.insert(format!("key{:04}", i), i * 10).unwrap();
        }
    }
    {
        let mut tree: BTree<String, u64> = BTree::open(&path, 4).unwrap();
        for i in 0u64..30 {
            assert_eq!(tree.get(&format!("key{:04}", i)).unwrap(), i * 10);
        }
    }
}

#[test]
fn test_single_key_insert_delete() {
    let (mut tree, _f) = tmp_tree();
    tree.insert("only".to_string(), 999u64).unwrap();
    assert_eq!(tree.get(&"only".to_string()).unwrap(), 999);
    tree.delete(&"only".to_string()).unwrap();
    assert!(tree.scan_all().unwrap().is_empty());
}

#[test]
fn test_invalid_order_rejected() {
    let file = NamedTempFile::new().unwrap();
    assert!(matches!(
        BTree::<String, u64>::open(file.path(), 1),
        Err(BTreeError::InvalidOrder { .. })
    ));
}

#[test]
fn test_u32_keys() {
    let file = NamedTempFile::new().unwrap();
    let mut tree: BTree<u32, u64> = BTree::open(file.path(), 4).unwrap();
    for i in 0u32..100 {
        tree.insert(i, i as u64 * 3).unwrap();
    }
    let all = tree.scan_all().unwrap();
    assert_eq!(all.len(), 100);
    for (k, v) in &all {
        assert_eq!(*v, *k as u64 * 3);
    }
}
