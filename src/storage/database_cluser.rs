use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::storage::disk::StorageManager;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DatabaseId(pub u64);

pub struct DatabaseCluster {
    /// Root directroy of the cluster
    cluster_dir: PathBuf,

    /// Default page size for the new database (can be overridden per-db later)
    default_page_size: usize,

    /// Open storage managers, keyed by database OID
    /// Wrapped in Arc<Mutex<<>> so we can share & mutate across threads laters
    databases: Mutex<HashMap<DatabaseId, Arc<Mutex<StorageManager>>>>,
}

impl DatabaseCluster {
    /// Create a new cluster manager.
    /// The directroy must exist or be creatable
    pub fn new(cluster_dir: impl AsRef<Path>, default_page_size: usize) -> io::Result<Self> {
        let cluster_path = cluster_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&cluster_path)?;
        std::fs::create_dir_all(cluster_path.join("base"))?;

        Ok(Self {
            cluster_dir: cluster_path,
            default_page_size,
            databases: Mutex::new(HashMap::new()),
        })
    }

    /// Open (or create) a database by its OID.
    /// Returns a shared reference to the StorageManager.
    /// If already open -> returns exsisting instance
    pub fn open_database(&self, db_id: DatabaseId) -> io::Result<Arc<Mutex<StorageManager>>> {
        let mut dbs = self.databases.lock().unwrap();

        if let Some(existing) = dbs.get(&db_id) {
            return Ok(existing.clone());
        }

        let db_dir = self.cluster_dir.join("base").join(db_id.0.to_string());
        std::fs::create_dir_all(&db_dir)?;

        let data_path = db_dir.join("database.data");

        let manager = StorageManager::open(data_path, self.default_page_size)?;

        let arc_manager = Arc::new(Mutex::new(manager));
        dbs.insert(db_id, arc_manager.clone());

        Ok(arc_manager)
    }

    // Convnience: open by database name -> but needs name -> oid mapping
    // (For now we keep it oid-based; catalog comes later)
    // pub fn open_database_by_name(&self, name: &str) -> io::Result<..>
    
    /// Close / drop a specific database's manager (optional, mostly for testing)
    pub fn close_database(&self, db_id: DatabaseId) {
        let mut dbs = self.databases.lock().unwrap();
        dbs.remove(&db_id);
    }

    pub fn list_open_databses(&self) -> Vec<DatabaseId> {
        let dbs = self. databases.lock().unwrap();
        dbs.keys().cloned().collect()
    }

    pub fn cluster_dir(&self) -> &Path {
        &self.cluster_dir
    }
}
