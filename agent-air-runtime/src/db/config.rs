//! Database configuration.

/// Configuration for the LMDB database environment.
#[derive(Debug, Clone)]
pub struct DbConfig {
    /// Maximum size of the memory map (default: 10 MiB).
    pub map_size: usize,
    /// Maximum number of named databases (default: 32).
    pub max_dbs: u32,
}

const DEFAULT_MAP_SIZE: usize = 10 * 1024 * 1024; // 10 MiB
const DEFAULT_MAX_DBS: u32 = 32;

impl Default for DbConfig {
    fn default() -> Self {
        Self {
            map_size: DEFAULT_MAP_SIZE,
            max_dbs: DEFAULT_MAX_DBS,
        }
    }
}
