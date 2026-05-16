//! Page manager: maps page IDs to fixed-size slots in a file.
//!
//! Layout:
//!   [0..8]   : u64  — next free page ID (also total page count)
//!   [8..16]  : u64  — root page ID
//!   [PAGE_SIZE * (id+1) .. PAGE_SIZE * (id+2)] : page data
//!
//! We keep a small dirty-page write-back cache so callers can
//! mutate nodes in memory and flush explicitly.

use crate::error::{BTreeError, Result};
use crate::node::BTreeNode;
use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

pub const PAGE_SIZE: usize = 4096;
const HEADER_SIZE: u64 = 16; // [next_page_id: u64][root_page_id: u64]
const NULL_PAGE: u64 = u64::MAX;

pub struct Pager {
    file: File,
    pub next_page_id: u64,
    pub root_page_id: u64,
    /// In-memory cache of serialized pages (page_id → bytes).
    cache: HashMap<u64, Vec<u8>>,
}

impl Pager {
    /// Open (or create) the pager file at `path`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)?;

        let (next_page_id, root_page_id) = read_header(&mut file)?;
        Ok(Pager {
            file,
            next_page_id,
            root_page_id,
            cache: HashMap::new(),
        })
    }

    /// Allocate a new page and return its ID.
    pub fn alloc_page(&mut self) -> Result<u64> {
        let id = self.next_page_id;
        self.next_page_id += 1;
        Ok(id)
    }

    /// Read and deserialize a node from `page_id`.
    pub fn read_node<K, V>(&mut self, page_id: u64) -> Result<BTreeNode<K, V>>
    where
        K: DeserializeOwned,
        V: DeserializeOwned,
    {
        let bytes = self.read_raw(page_id)?;
        let node: BTreeNode<K, V> = bincode::deserialize(&bytes)?;
        Ok(node)
    }

    /// Serialize `node` and write it to `page_id` (goes to cache, flushed on demand).
    pub fn write_node<K, V>(&mut self, page_id: u64, node: &BTreeNode<K, V>) -> Result<()>
    where
        K: Serialize,
        V: Serialize,
    {
        let bytes = bincode::serialize(node)?;
        if bytes.len() > PAGE_SIZE {
            return Err(BTreeError::CorruptPage { page_id });
        }
        self.cache.insert(page_id, bytes);
        Ok(())
    }

    /// Flush all dirty pages and the header to disk.
    pub fn flush(&mut self) -> Result<()> {
        // Write dirty pages.
        let pages: Vec<(u64, Vec<u8>)> = self.cache.drain().collect();
        for (page_id, bytes) in pages {
            self.write_raw(page_id, &bytes)?;
        }
        // Write header.
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&self.next_page_id.to_le_bytes())?;
        self.file.write_all(&self.root_page_id.to_le_bytes())?;
        self.file.flush()?;
        Ok(())
    }

    // ── private helpers ──────────────────────────────────────────────────────

    fn offset(page_id: u64) -> u64 {
        HEADER_SIZE + page_id * PAGE_SIZE as u64
    }

    fn read_raw(&mut self, page_id: u64) -> Result<Vec<u8>> {
        // Serve from cache if available.
        if let Some(cached) = self.cache.get(&page_id) {
            return Ok(cached.clone());
        }
        let offset = Self::offset(page_id);
        self.file.seek(SeekFrom::Start(offset))?;
        let mut buf = vec![0u8; PAGE_SIZE];
        self.file.read_exact(&mut buf)?;
        Ok(buf)
    }

    fn write_raw(&mut self, page_id: u64, bytes: &[u8]) -> Result<()> {
        let mut buf = vec![0u8; PAGE_SIZE];
        let len = bytes.len().min(PAGE_SIZE);
        buf[..len].copy_from_slice(&bytes[..len]);
        let offset = Self::offset(page_id);
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.write_all(&buf)?;
        Ok(())
    }
}

fn read_header(file: &mut File) -> Result<(u64, u64)> {
    let meta = file.metadata()?;
    if meta.len() < HEADER_SIZE {
        // New file — write zeroed header.
        file.seek(SeekFrom::Start(0))?;
        file.write_all(&0u64.to_le_bytes())?; // next_page_id
        file.write_all(&NULL_PAGE.to_le_bytes())?; // root_page_id (none yet)
        file.flush()?;
        return Ok((0, NULL_PAGE));
    }
    file.seek(SeekFrom::Start(0))?;
    let mut buf = [0u8; 16];
    file.read_exact(&mut buf)?;
    let next = u64::from_le_bytes(buf[0..8].try_into().unwrap());
    let root = u64::from_le_bytes(buf[8..16].try_into().unwrap());
    Ok((next, root))
}

pub const NO_PAGE: u64 = NULL_PAGE;
