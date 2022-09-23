use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::data_store::{BlockId, BlockKey, BlockType, DataStore};
use super::error::Result;
use super::open_store::OpenStore;

#[derive(Debug, Clone, Default)]
struct BlockMap {
    data: HashMap<BlockId, Vec<u8>>,
    locks: HashMap<BlockId, Vec<u8>>,
    headers: HashMap<BlockId, Vec<u8>>,
    superblock: Option<Vec<u8>>,
    version: Option<Vec<u8>>,
}

/// The configuration for opening a [`MemoryStore`].
///
/// [`MemoryStore`]: crate::store::MemoryStore
#[derive(Debug, Clone, Default)]
pub struct MemoryConfig {
    blocks: Arc<Mutex<BlockMap>>,
}

impl MemoryConfig {
    /// Create a new empty `MemoryConfig`.
    pub fn new() -> Self {
        Self {
            blocks: Arc::new(Mutex::new(BlockMap::default())),
        }
    }
}

impl OpenStore for MemoryConfig {
    type Store = MemoryStore;

    fn open(&self) -> crate::Result<Self::Store> {
        Ok(MemoryStore {
            blocks: Arc::clone(&self.blocks),
        })
    }
}

/// A `DataStore` which stores data in memory.
///
/// Unlike other `DataStore` implementations, data in a `MemoryStore` is not stored persistently
/// and is only accessible to the current process. This data store is useful for testing.
///
/// None of the methods in this data store will ever return `Err`.
///
/// You can use [`MemoryConfig`] to open a data store of this type.
///
/// [`MemoryConfig`]: crate::store::MemoryConfig
#[derive(Debug)]
pub struct MemoryStore {
    blocks: Arc<Mutex<BlockMap>>,
}

impl DataStore for MemoryStore {
    fn write_block(&mut self, key: BlockKey, data: &[u8]) -> Result<()> {
        let mut block_map = self.blocks.lock().unwrap();
        match key {
            BlockKey::Data(id) => {
                block_map.data.insert(id, data.to_owned());
            }
            BlockKey::Lock(id) => {
                block_map.locks.insert(id, data.to_owned());
            }
            BlockKey::Header(id) => {
                block_map.headers.insert(id, data.to_owned());
            }
            BlockKey::Super => {
                block_map.superblock = Some(data.to_owned());
            }
            BlockKey::Version => {
                block_map.version = Some(data.to_owned());
            }
        }
        Ok(())
    }

    fn read_block(&mut self, key: BlockKey, buf: &mut Vec<u8>) -> Result<Option<usize>> {
        let block_map = self.blocks.lock().unwrap();
        let maybe_bytes = match key {
            BlockKey::Data(id) => block_map.data.get(&id).map(|block| block.as_slice()),
            BlockKey::Lock(id) => block_map.locks.get(&id).map(|block| block.as_slice()),
            BlockKey::Header(id) => block_map.headers.get(&id).map(|block| block.as_slice()),
            BlockKey::Super => block_map.superblock.as_deref(),
            BlockKey::Version => block_map.version.as_deref(),
        };

        if let Some(bytes) = maybe_bytes {
            buf.extend_from_slice(bytes);
            Ok(Some(bytes.len()))
        } else {
            Ok(None)
        }
    }

    fn remove_block(&mut self, key: BlockKey) -> Result<()> {
        let mut block_map = self.blocks.lock().unwrap();
        match key {
            BlockKey::Data(id) => {
                block_map.data.remove(&id);
            }
            BlockKey::Lock(id) => {
                block_map.locks.remove(&id);
            }
            BlockKey::Header(id) => {
                block_map.headers.remove(&id);
            }
            BlockKey::Super => {
                block_map.superblock = None;
            }
            BlockKey::Version => {
                block_map.version = None;
            }
        }
        Ok(())
    }

    fn list_blocks(&mut self, kind: BlockType, list: &mut Vec<BlockId>) -> Result<()> {
        let block_map = self.blocks.lock().unwrap();
        Ok(match kind {
            BlockType::Data => list.extend(block_map.data.keys().copied()),
            BlockType::Lock => list.extend(block_map.locks.keys().copied()),
            BlockType::Header => list.extend(block_map.headers.keys().copied()),
        })
    }
}
