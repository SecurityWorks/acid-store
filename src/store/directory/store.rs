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

use std::fs::{create_dir_all, File, remove_dir_all, remove_file, rename};
use std::io::{self, Read, Write};
use std::path::PathBuf;

use uuid::Uuid;
use walkdir::WalkDir;

use crate::store::DataStore;

/// A UUID which acts as the version ID of the directory store format.
const CURRENT_VERSION: &str = "2891c3da-297e-11ea-a7c9-1b8f8be4fc9b";

/// A `DataStore` which stores data in a directory in the local file system.
pub struct DirectoryStore {
    /// The path of the store's root directory.
    path: PathBuf,

    /// The path of the directory where blocks are stored.
    blocks_directory: PathBuf,

    /// The path of the directory were blocks are staged while being written to.
    staging_directory: PathBuf,
}

impl DirectoryStore {
    /// Create a new directory store at the given `path`.
    ///
    /// # Errors
    /// - `ErrorKind::AlreadyExists`: There is already a file at the given path.
    /// - `ErrorKind::PermissionDenied`: The user lacks permissions to create the directory.
    pub fn create(path: PathBuf) -> io::Result<Self> {
        create_dir_all(path)?;
        let mut version_file = File::create(path.join("version"))?;
        version_file.write_all(CURRENT_VERSION.as_bytes());
        Self::open(path)
    }

    /// Open an existing directory store at `path`.
    ///
    /// # Errors
    /// - `ErrorKind::NotFound`: There is not a directory at `path`.
    /// - `ErrorKind::InvalidData`: The directory at `path` is not a valid directory store.
    /// - `ErrorKind::PermissionDenied`: The user lacks permissions to read the directory.
    pub fn open(path: PathBuf) -> io::Result<Self> {
        let mut version_file = File::open(path.join("version"))?;
        let mut version_id = String::new();
        version_file.read_to_string(&mut version_id)?;

        if version_id != CURRENT_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "The directory is not a valid directory store.",
            ));
        }

        Ok(DirectoryStore {
            path,
            blocks_directory: path.join("blocks"),
            staging_directory: path.join("stage")
        })
    }

    /// Return the path where a block with the given `id` will be stored.
    fn block_path(&self, id: &Uuid) -> PathBuf {
        let hex = id.to_simple().encode_lower(&mut Uuid::encode_buffer());
        self.blocks_directory.join(&hex[..2]).join(hex)
    }

    /// Return the path where a block with the given `id` will be staged.
    fn staging_path(&self, id: &Uuid) -> PathBuf {
        let hex = id.to_simple().encode_lower(&mut Uuid::encode_buffer());
        self.staging_directory.join(hex)
    }
}

impl DataStore for DirectoryStore {
    fn write_block(&mut self, id: &Uuid, data: &[u8]) -> io::Result<()> {
        let staging_path = self.staging_path(id);
        let block_path = self.block_path(id);
        create_dir_all(staging_path.parent().unwrap())?;
        create_dir_all(block_path.parent().unwrap())?;

        // Write to a staging file and then atomically move it to its final destination.
        let mut staging_file = File::create(staging_path)?;
        staging_file.write_all(data)?;
        rename(staging_path, block_path)?;

        // Remove any unused staging files.
        remove_dir_all(self.staging_directory)?;

        Ok(())
    }

    fn read_block(&self, id: &Uuid) -> io::Result<Vec<u8>> {
        let block_path = self.block_path(id);

        if block_path.exists() {
            let mut file = File::open(block_path)?;
            let mut buffer = Vec::with_capacity(file.metadata()?.len() as usize);
            file.read_to_end(&mut buffer)?;
            Ok(buffer)
        } else {
            panic!("There is no block with the given ID.")
        }
    }

    fn remove_block(&mut self, id: &Uuid) -> io::Result<()> {
        remove_file(self.block_path(id))
    }

    fn list_blocks(&self) -> io::Result<Box<dyn Iterator<Item=io::Result<Uuid>>>> {
        Ok(Box::new(
            WalkDir::new(self.blocks_directory)
                .min_depth(2)
                .into_iter()
                .map(|result| match result {
                    Ok(entry) => Ok(Uuid::parse_str(
                        entry
                            .file_name()
                            .to_str()
                            .expect("Block file name is invalid."),
                    )
                        .expect("Block file name is invalid.")),
                    Err(error) => Err(io::Error::from(error)),
                }),
        ))
    }
}
