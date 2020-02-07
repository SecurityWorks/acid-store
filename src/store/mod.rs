/*
 * Copyright 2019-2020 Garrett Powell
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

//! Low-level backends for data storage.
//!
//! # Examples
//! Open a data store which stores data in a directory of the local file system. Create the data
//! store if it doesn't already exist, and truncate it if it does.
//! ```no_run
//! use acid_store::store::{DirectoryStore, Open, OpenOption};
//!
//! let store = DirectoryStore::open(
//!     "/home/lostatc/store".into(),
//!     OpenOption::CREATE | OpenOption::TRUNCATE
//! ).unwrap();
//! ```

pub use self::common::{DataStore, Open, OpenOption};
#[cfg(feature = "store-directory")]
pub use self::directory::DirectoryStore;
pub use self::memory::MemoryStore;
#[cfg(feature = "store-redis")]
pub use self::redis::RedisStore;
#[cfg(feature = "store-s3")]
pub use self::s3::S3Store;
#[cfg(feature = "store-sqlite")]
pub use self::sqlite::SqliteStore;

mod common;
mod directory;
mod memory;
mod redis;
mod s3;
mod sqlite;
