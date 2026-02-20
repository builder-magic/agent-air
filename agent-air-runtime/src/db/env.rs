//! LMDB environment wrapper.

use std::path::Path;
use std::sync::Arc;

use heed::EnvOpenOptions;
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::config::DbConfig;
use super::error::DbError;
use super::store::Store;

/// Wrapper around an LMDB environment.
///
/// Cloneable via internal `Arc`. Call [`AgentDatabase::open`] to create,
/// then [`AgentDatabase::store`] to obtain typed key-value accessors.
#[derive(Clone)]
pub struct AgentDatabase {
    env: Arc<heed::Env<heed::WithoutTls>>,
}

impl AgentDatabase {
    /// Open (or create) an LMDB environment at `data_dir`.
    ///
    /// The directory is created if it does not exist. The environment is
    /// configured with `read_txn_without_tls()` so read transactions can
    /// be used from multiple tokio worker threads.
    pub fn open(data_dir: &Path, config: &DbConfig) -> Result<Self, DbError> {
        std::fs::create_dir_all(data_dir).map_err(|e| DbError::Env(heed::Error::Io(e)))?;

        let mut opts = EnvOpenOptions::new();
        opts.map_size(config.map_size);
        opts.max_dbs(config.max_dbs);
        let env = unsafe { opts.read_txn_without_tls().open(data_dir)? };

        Ok(Self { env: Arc::new(env) })
    }

    /// Create a typed [`Store`] backed by a named LMDB database.
    ///
    /// The named database is created if it does not already exist.
    pub fn store<V>(&self, name: &str) -> Result<Store<V>, DbError>
    where
        V: Serialize + DeserializeOwned + Send + Sync + 'static,
    {
        let mut wtxn = self.env.write_txn()?;
        let db = self
            .env
            .create_database::<heed::types::Str, heed::types::SerdeJson<V>>(
                &mut wtxn,
                Some(name),
            )?;
        wtxn.commit()?;

        Ok(Store::new(self.env.clone(), db))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_creates_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("subdir");
        assert!(!db_path.exists());

        let _db = AgentDatabase::open(&db_path, &DbConfig::default()).unwrap();
        assert!(db_path.exists());
    }

    #[test]
    fn two_named_stores_are_independent() {
        let tmp = tempfile::tempdir().unwrap();
        let db = AgentDatabase::open(tmp.path(), &DbConfig::default()).unwrap();

        let store_a = db.store::<String>("a").unwrap();
        let store_b = db.store::<String>("b").unwrap();

        store_a.put("key", &"from_a".to_string()).unwrap();
        store_b.put("key", &"from_b".to_string()).unwrap();

        assert_eq!(store_a.get("key").unwrap().unwrap(), "from_a");
        assert_eq!(store_b.get("key").unwrap().unwrap(), "from_b");
    }
}
