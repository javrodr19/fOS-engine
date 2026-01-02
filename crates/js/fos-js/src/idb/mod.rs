//! Storage APIs
//!
//! IndexedDB, Cache API, Cookies.

pub mod indexeddb;
pub mod cache_api;
pub mod cookies;

pub use indexeddb::{IDBFactory, IDBDatabase, IDBObjectStore, IDBKey, IDBValue, IDBKeyRange};
pub use cache_api::{CacheStorage, Cache, CacheRequest, CacheResponse};
pub use cookies::{CookieStore, Cookie, SameSite};
