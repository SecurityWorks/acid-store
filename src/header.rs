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

use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::mem::size_of;
use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;
use rmp_serde::{decode, encode};
use serde::{Deserialize, Serialize};

use crate::block::{BLOCK_OFFSET, BlockAddress, pad_to_block_size};
use crate::error::Result;
use crate::serialization::SerializableNaiveDateTime;

/// The size of the checksum of each file.
pub const FILE_HASH_SIZE: usize = 32;

/// The checksum of a file.
pub type FileChecksum = [u8; FILE_HASH_SIZE];

/// A type of file which can be stored in an archive.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryType {
    /// A regular file with opaque contents.
    File {
        /// The size of the file in bytes.
        size: u64,

        /// The BLAKE2 checksum of the file.
        checksum: FileChecksum,

        /// The locations of blocks containing the data for this file.
        blocks: Vec<BlockAddress>,
    },

    /// A directory.
    Directory,

    /// A symbolic link.
    Link {
        /// The path of the target of this symbolic link.
        target: PathBuf
    },
}

/// An extended attribute of a file.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtendedAttribute {
    /// The name of the attribute.
    pub name: String,

    /// The value of the attribute.
    pub value: Vec<u8>,
}

/// Metadata about a file which is stored in an archive.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveEntry {
    /// The path of the file in the archive.
    pub path: PathBuf,

    /// The time the file was last modified.
    #[serde(with = "SerializableNaiveDateTime")]
    pub modified_time: NaiveDateTime,

    /// The POSIX permissions bits of the file, or `None` if POSIX permissions are not applicable.
    pub permissions: Option<i32>,

    /// The file's extended attributes.
    pub attributes: Vec<ExtendedAttribute>,

    /// The type of file this entry represents.
    pub entry_type: EntryType,
}

/// Metadata about files stored in the archive.
#[derive(Debug, Serialize, Deserialize)]
pub struct Header {
    /// The entries which are stored in this archive.
    pub entries: Vec<ArchiveEntry>,
}

impl Header {
    /// Returns the set of locations of blocks used for storing data.
    fn data_blocks(&self) -> Vec<BlockAddress> {
        self.entries
            .iter()
            .filter_map(|entry| match &entry.entry_type {
                EntryType::File { blocks, .. } => Some(blocks),
                _ => None
            })
            .flatten()
            .copied()
            .collect()
    }

    /// Returns a list of addresses of blocks which are unused and can be overwritten.
    pub fn unused_blocks(&self, location: &HeaderAddress) -> Vec<BlockAddress> {
        let mut used_blocks = HashSet::new();
        used_blocks.extend(self.data_blocks());
        used_blocks.extend(location.header_blocks());

        let mut unused_blocks = location.blocks();
        unused_blocks.retain(|block| !used_blocks.contains(block));

        unused_blocks
    }

    /// Reads the header from the given `archive`.
    ///
    /// # Errors
    /// - `Error::Io`: An I/O error occurred reading from the archive.
    /// - `Error::Deserialize`: An error occurred deserializing the header.
    pub fn read(archive: &Path) -> Result<(Header, HeaderAddress)> {
        let mut file = File::open(archive)?;
        let mut offset_buffer = [0u8; size_of::<u64>()];
        let archive_size = file.metadata()?.len();

        // Get the offset of the header.
        file.seek(SeekFrom::Start(0))?;
        file.read_exact(&mut offset_buffer)?;
        let offset = u64::from_be_bytes(offset_buffer);

        // Read the header size and header.
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(&mut offset_buffer)?;
        let header_size = u64::from_be_bytes(offset_buffer);
        let header = decode::from_read(file.take(header_size))?;

        let header_address = HeaderAddress { offset, header_size, archive_size };
        Ok((header, header_address))
    }

    /// Writes this header to the given `archive` and returns its address.
    ///
    /// This does not overwrite the old header, but instead marks the space as unused so that it can
    /// be overwritten with new data in the future. If this method call is interrupted before the
    /// header is fully written, the old header will still be valid and the written bytes of the new
    /// header will be marked as unused.
    ///
    /// # Errors
    /// - `Error::Io`: An I/O error occurred writing to the archive.
    /// - `Error::Serialize`: An error occurred serializing the header.
    pub fn write(&self, archive: &Path) -> Result<HeaderAddress> {
        let mut file = File::open(archive)?;

        // Pad the file to a multiple of `BLOCK_SIZE`.
        let offset = file.seek(SeekFrom::End(0))?;
        pad_to_block_size(&mut file)?;

        // Append the new header size and header.
        let serialized_header = encode::to_vec(&self)?;
        file.write_all(&serialized_header.len().to_be_bytes())?;
        file.write_all(&serialized_header)?;

        // Update the header offset to point to the new header.
        file.seek(SeekFrom::Start(0))?;
        file.write_all(&offset.to_be_bytes())?;

        let archive_size = file.metadata()?.len();
        let header_size = archive_size - offset;

        Ok(HeaderAddress { offset, header_size, archive_size })
    }
}

/// The address of the header in the archive.
#[derive(Debug, PartialEq, Eq)]
pub struct HeaderAddress {
    /// The offset of the first block in the header.
    offset: u64,

    /// The size of the header in bytes.
    header_size: u64,

    /// The size of the archive in bytes.
    archive_size: u64,
}

impl HeaderAddress {
    /// Returns the list of addresses of all blocks in the archive.
    fn blocks(&self) -> Vec<BlockAddress> {
        BlockAddress::range(BLOCK_OFFSET, self.archive_size)
    }

    /// Returns the list of addresses of blocks used for storing the header.
    fn header_blocks(&self) -> Vec<BlockAddress> {
        BlockAddress::range(self.offset, self.header_size)
    }
}
