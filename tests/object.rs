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

use std::fs::metadata;
use std::io;

use rand::{RngCore, SeedableRng};
use rand::rngs::SmallRng;
use tempfile::tempdir;

use disk_archive::{ArchiveConfig, Compression, Encryption, ObjectArchive};

/// Return a buffer containing `size` random bytes for testing purposes.
fn random_bytes(size: usize) -> Vec<u8> {
    let mut rng = SmallRng::from_entropy();
    let mut buffer = vec![0u8; size];
    rng.fill_bytes(&mut buffer);
    buffer
}

/// Insert random data into the `archive` with the given `key`.
fn insert_data(key: &str, archive: &mut ObjectArchive<String>) -> io::Result<Vec<u8>> {
    let data = random_bytes(DATA_SIZE);
    let object = archive.write(data.as_slice())?;
    archive.insert(key.to_string(), object);
    Ok(data)
}

/// Retrieve the data in the `archive` associated with the given `key`.
fn read_data(key: &str, archive: &ObjectArchive<String>) -> io::Result<Vec<u8>> {
    let object = archive.get(&key.to_string()).unwrap();
    archive.read_all(&object)
}

/// The size of a test data buffer.
///
/// This is considerably larger than the archive block size, but not an exact multiple of it.
const DATA_SIZE: usize = (1024 * 1024 * 4) + 200;

/// The archive config to use for testing.
const ARCHIVE_CONFIG: ArchiveConfig = ArchiveConfig {
    block_size: 4096,
    chunker_bits: 18,
    encryption: Encryption::None,
    compression: Compression::None
};

// TODO: Use macros to generate similar tests.

#[test]
fn object_is_persisted() -> io::Result<()> {
    let temp_dir = tempdir()?;
    let archive_path = temp_dir.path().join("archive");
    let mut archive = ObjectArchive::create(archive_path.as_path(), ARCHIVE_CONFIG, None)?;

    let expected_data = insert_data("Test", &mut archive)?;

    archive.commit()?;
    drop(archive);

    let archive = ObjectArchive::open(archive_path.as_path(), None)?;

    let actual_data = read_data("Test", &archive)?;

    assert_eq!(expected_data, actual_data);

    Ok(())
}

#[test]
fn multiple_objects_are_persisted() -> io::Result<()> {
    let temp_dir = tempdir()?;
    let archive_path = temp_dir.path().join("archive");
    let mut archive = ObjectArchive::create(archive_path.as_path(), ARCHIVE_CONFIG, None)?;

    let expected_data1 = insert_data("Test1", &mut archive)?;
    let expected_data2 = insert_data("Test2", &mut archive)?;
    let expected_data3 = insert_data("Test3", &mut archive)?;

    archive.commit()?;
    drop(archive);

    let archive = ObjectArchive::open(archive_path.as_path(), None)?;

    let actual_data1 = read_data("Test1", &archive)?;
    let actual_data2 = read_data("Test2", &archive)?;
    let actual_data3 = read_data("Test3", &archive)?;

    assert_eq!(expected_data1, actual_data1);
    assert_eq!(expected_data2, actual_data2);
    assert_eq!(expected_data3, actual_data3);

    Ok(())
}

#[test]
fn removed_objects_are_overwritten() -> io::Result<()> {
    let temp_dir = tempdir()?;
    let archive_path = temp_dir.path().join("archive");
    let mut archive = ObjectArchive::create(archive_path.as_path(), ARCHIVE_CONFIG, None)?;

    insert_data("Test1", &mut archive)?;

    archive.commit()?;
    archive.remove(&"Test1".to_string());
    archive.commit()?;

    insert_data("Test2", &mut archive)?;

    archive.commit()?;
    drop(archive);

    let archive_size = metadata(archive_path)?.len();
    assert!(archive_size < (DATA_SIZE * 2) as u64);

    Ok(())
}

#[test]
fn uncommitted_changes_are_not_saved() -> io::Result<()> {
    let temp_dir = tempdir()?;
    let archive_path = temp_dir.path().join("archive");
    let mut archive = ObjectArchive::create(archive_path.as_path(), Default::default(), None)?;

    insert_data("Test", &mut archive)?;

    drop(archive);

    let archive = ObjectArchive::open(archive_path.as_path(), None)?;

    assert_eq!(archive.get(&"Test".to_string()), None);

    Ok(())
}
