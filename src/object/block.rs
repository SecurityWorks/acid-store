/*
 * Copyright 2019 Garrett Powell
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::mem::size_of;

use rmp_serde::{from_read, to_vec};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::compression::Compression;
use super::config::ArchiveConfig;
use super::encryption::Encryption;

/// The offset of the primary superblock from the start of the file.
const PRIMARY_SUPERBLOCK_OFFSET: u64 = 0;

/// The offset of the backup superblock from the start of the file.
const BACKUP_SUPERBLOCK_OFFSET: u64 = 4096;

/// The length of a superblock.
const SUPERBLOCK_SIZE: usize = 4096;

/// The number of bytes reserved for the superblock and its backup.
const RESERVED_SPACE: u64 = SUPERBLOCK_SIZE as u64 * 2;

/// Appends to the given `file` to pad it to a multiple of `block_size`.
///
/// This returns the new size of the file.
pub fn pad_to_block_size(mut file: &File, block_size: u32) -> io::Result<u64> {
    let position = file.seek(SeekFrom::End(0))?;
    let padding_size = block_size as u64 - ((position - RESERVED_SPACE) % block_size as u64);
    let padding = vec![0u8; padding_size as usize];
    file.write_all(&padding)?;

    Ok(position + padding_size)
}

/// A sequence of contiguous blocks in the archive.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub struct Extent {
    /// The index of the first block in the extent.
    pub index: u64,

    /// The number of blocks in the extent.
    pub blocks: u64,
}

impl Extent {
    /// The offset of start of the extent from the start of the archive in bytes.
    pub fn start(&self, block_size: u32) -> u64 {
        RESERVED_SPACE + (self.index * block_size as u64)
    }

    /// The offset of end of the extent from the start of the archive in bytes.
    pub fn end(&self, block_size: u32) -> u64 {
        self.start(block_size) + self.length(block_size)
    }

    /// The length of the extent in bytes.
    pub fn length(&self, block_size: u32) -> u64 {
        self.blocks * block_size as u64
    }

    /// Returns the extent that is between this extent and `other`, or `None` if they are adjacent.
    pub fn between(&self, other: Extent) -> Option<Extent> {
        let new_extent = Extent {
            index: self.index + self.blocks,
            blocks: other.index - (self.index + self.blocks),
        };

        if new_extent.blocks <= 0 {
            None
        } else {
            Some(new_extent)
        }
    }
}

/// A chunk of data in an archive.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct Chunk {
    /// The total size of the chunk in bytes.
    pub size: u64,

    /// The extents containing the data for this chunk.
    pub extents: Vec<Extent>,
}

/// The archive's superblock.
///
/// This stores unencrypted metadata about the archive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuperBlock {
    /// The unique ID of this archive.
    pub id: Uuid,

    /// The block size of the archive in bytes.
    pub block_size: u32,

    /// The number of bits that define a chunk boundary.
    ///
    /// The average size of a chunk will be 2^`chunker_bits` bytes.
    pub chunker_bits: u32,

    /// The compression method being used in this archive.
    pub compression: Compression,

    /// The encryption method being used in this archive.
    pub encryption: Encryption,

    /// The extent which stores the archive's header.
    pub header: Extent,

    /// The size of the header in bytes.
    pub header_size: u32,
}

impl SuperBlock {
    /// Read the superblock from the given `file` stored at the given `offset`.
    ///
    /// # Errors
    /// - `Error::Io`: An I/O error occurred.
    /// - `Error::Deserialize`: The superblock could not be deserialized.
    fn read_at(file: &mut File, offset: u64) -> io::Result<Self> {
        file.seek(SeekFrom::Start(offset))?;

        // Get the size of the superblock.
        let mut size_buffer = [0u8; size_of::<u32>()];
        file.read_exact(&mut size_buffer)?;
        let superblock_size = u32::from_be_bytes(size_buffer) as u64;

        // Deserialize the superblock.
        let superblock = from_read(file.take(superblock_size)).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "The superblock is corrupt.")
        })?;

        Ok(superblock)
    }

    /// Write the superblock to the given `file` at the given `offset`.
    fn write_at(&self, file: &mut File, offset: u64) -> io::Result<()> {
        file.seek(SeekFrom::Start(offset))?;

        // Serialize the superblock.
        let superblock = to_vec(&self).expect("Could not serialize the superblock.");
        let superblock_size = superblock.len() as u32;

        // We need to pad the superblock to ensure that the space is allocated when an archive is
        // first created.
        let padding_size = SUPERBLOCK_SIZE - superblock_size as usize - size_of::<u32>();
        let padding = vec![0u8; padding_size];

        // Write the superblock size and the superblock itself.
        file.write_all(&superblock_size.to_be_bytes())?;
        file.write_all(&superblock)?;
        file.write_all(&padding)?;

        Ok(())
    }

    /// Read the superblock from the given `file` or the backup superblock if it is corrupt.
    pub fn read(file: &mut File) -> io::Result<Self> {
        // Read both the primary and backup superblock.
        let primary_superblock = Self::read_at(file, PRIMARY_SUPERBLOCK_OFFSET);
        let backup_superblock = Self::read_at(file, BACKUP_SUPERBLOCK_OFFSET);

        // If one of the superblocks is corrupt, repair it with the other before proceeding. We can
        // guarantee consistency because a superblock will only be written if the other is valid.
        return match (primary_superblock, backup_superblock) {
            (Ok(superblock), Ok(_)) => Ok(superblock),
            (Ok(superblock), Err(_)) => {
                superblock.write_at(file, BACKUP_SUPERBLOCK_OFFSET)?;
                Ok(superblock)
            }
            (Err(_), Ok(superblock)) => {
                superblock.write_at(file, PRIMARY_SUPERBLOCK_OFFSET)?;
                Ok(superblock)
            }
            (Err(primary_error), Err(backup_error)) => {
                // Neither superblock could be read.
                if primary_error.kind() == io::ErrorKind::InvalidData
                    && backup_error.kind() == io::ErrorKind::InvalidData
                {
                    Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Both the primary and backup superblock are corrupt. This is most likely unrecoverable.",
                    ))
                } else {
                    Err(primary_error)
                }
            }
        };
    }

    /// Write this superblock to the given `file` twice, a primary and a backup.
    pub fn write(&self, file: &mut File) -> io::Result<()> {
        self.write_at(file, PRIMARY_SUPERBLOCK_OFFSET)?;
        self.write_at(file, BACKUP_SUPERBLOCK_OFFSET)?;

        Ok(())
    }

    /// Returns the `ArchiveConfig` used to configure this archive.
    pub fn to_config(&self) -> ArchiveConfig {
        ArchiveConfig {
            block_size: self.block_size,
            chunker_bits: self.chunker_bits,
            encryption: self.encryption.clone(),
            compression: self.compression.clone(),
        }
    }
}
