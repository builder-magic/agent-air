//! Typed key-value store backed by a named LMDB database.

use std::sync::Arc;

use heed::Database;
use heed::types::{SerdeJson, Str};
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::error::DbError;

/// A typed key-value store.
///
/// Keys are `&str`. Values are serialized as JSON via `heed::types::SerdeJson`.
/// Obtain an instance through [`super::AgentDatabase::store`].
pub struct Store<V>
where
    V: Serialize + DeserializeOwned + Send + Sync + 'static,
{
    env: Arc<heed::Env<heed::WithoutTls>>,
    db: Database<Str, SerdeJson<V>>,
}

impl<V> Store<V>
where
    V: Serialize + DeserializeOwned + Send + Sync + 'static,
{
    pub(crate) fn new(
        env: Arc<heed::Env<heed::WithoutTls>>,
        db: Database<Str, SerdeJson<V>>,
    ) -> Self {
        Self { env, db }
    }

    /// Read a value by key. Returns `None` if the key does not exist.
    pub fn get(&self, key: &str) -> Result<Option<V>, DbError> {
        let rtxn = self.env.read_txn()?;
        let val = self.db.get(&rtxn, key)?;
        Ok(val)
    }

    /// Insert or update a value.
    pub fn put(&self, key: &str, value: &V) -> Result<(), DbError> {
        let mut wtxn = self.env.write_txn()?;
        self.db.put(&mut wtxn, key, value)?;
        wtxn.commit()?;
        Ok(())
    }

    /// Delete a key. Returns `true` if the key existed.
    pub fn delete(&self, key: &str) -> Result<bool, DbError> {
        let mut wtxn = self.env.write_txn()?;
        let existed = self.db.delete(&mut wtxn, key)?;
        wtxn.commit()?;
        Ok(existed)
    }

    /// Check whether a key exists.
    pub fn contains(&self, key: &str) -> Result<bool, DbError> {
        let rtxn = self.env.read_txn()?;
        let val = self.db.get(&rtxn, key)?;
        Ok(val.is_some())
    }

    /// Open a write transaction for batched writes.
    ///
    /// Use with [`Store::put_with_txn`] and commit via `txn.commit()`.
    pub fn write_txn(&self) -> Result<heed::RwTxn<'_>, DbError> {
        let wtxn = self.env.write_txn()?;
        Ok(wtxn)
    }

    /// Insert or update a value using an existing write transaction.
    pub fn put_with_txn(
        &self,
        txn: &mut heed::RwTxn<'_>,
        key: &str,
        value: &V,
    ) -> Result<(), DbError> {
        self.db.put(txn, key, value)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::config::DbConfig;
    use super::super::env::AgentDatabase;

    #[test]
    fn put_get_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let db = AgentDatabase::open(tmp.path(), &DbConfig::default()).unwrap();
        let store = db.store::<String>("test").unwrap();

        store.put("hello", &"world".to_string()).unwrap();
        let val = store.get("hello").unwrap();
        assert_eq!(val, Some("world".to_string()));
    }

    #[test]
    fn missing_key_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let db = AgentDatabase::open(tmp.path(), &DbConfig::default()).unwrap();
        let store = db.store::<String>("test").unwrap();

        let val = store.get("nonexistent").unwrap();
        assert_eq!(val, None);
    }

    #[test]
    fn delete_returns_correct_bool() {
        let tmp = tempfile::tempdir().unwrap();
        let db = AgentDatabase::open(tmp.path(), &DbConfig::default()).unwrap();
        let store = db.store::<String>("test").unwrap();

        store.put("key", &"val".to_string()).unwrap();
        assert!(store.delete("key").unwrap());
        assert!(!store.delete("key").unwrap());

        // Confirm the value is gone
        assert_eq!(store.get("key").unwrap(), None);
    }

    #[test]
    fn contains_key() {
        let tmp = tempfile::tempdir().unwrap();
        let db = AgentDatabase::open(tmp.path(), &DbConfig::default()).unwrap();
        let store = db.store::<String>("test").unwrap();

        assert!(!store.contains("k").unwrap());
        store.put("k", &"v".to_string()).unwrap();
        assert!(store.contains("k").unwrap());
    }

    #[test]
    fn batch_write_with_txn() {
        let tmp = tempfile::tempdir().unwrap();
        let db = AgentDatabase::open(tmp.path(), &DbConfig::default()).unwrap();
        let store = db.store::<i64>("batch").unwrap();

        let mut txn = store.write_txn().unwrap();
        store.put_with_txn(&mut txn, "a", &1).unwrap();
        store.put_with_txn(&mut txn, "b", &2).unwrap();
        store.put_with_txn(&mut txn, "c", &3).unwrap();
        txn.commit().unwrap();

        assert_eq!(store.get("a").unwrap(), Some(1));
        assert_eq!(store.get("b").unwrap(), Some(2));
        assert_eq!(store.get("c").unwrap(), Some(3));
    }
}
