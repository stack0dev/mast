pub mod fulltext;
pub mod metadata;
pub mod vector;

use crate::error::MastError;
use crate::storage::Storage;
use std::collections::HashMap;
use vector::VectorIndex;

pub struct IndexManager {
    indexes: HashMap<String, VectorIndex>,
}

impl IndexManager {
    pub fn open(storage: &Storage) -> Result<Self, MastError> {
        let mut indexes = HashMap::new();

        let collections = storage.list_collections()?;
        for col in &collections {
            if let Some(buffer) = storage.read_vector_index(&col.name)? {
                let index = VectorIndex::from_buffer(&buffer, col.dimensions)?;
                indexes.insert(col.name.clone(), index);
            }
        }

        Ok(Self { indexes })
    }

    pub fn get_or_create(
        &mut self,
        collection: &str,
        dimensions: usize,
    ) -> Result<&mut VectorIndex, MastError> {
        if !self.indexes.contains_key(collection) {
            let index = VectorIndex::new(dimensions)?;
            self.indexes.insert(collection.to_string(), index);
        }
        Ok(self.indexes.get_mut(collection).unwrap())
    }

    pub fn get(&self, collection: &str) -> Option<&VectorIndex> {
        self.indexes.get(collection)
    }

    pub fn flush_all(&self, storage: &Storage) -> Result<(), MastError> {
        for (collection, index) in &self.indexes {
            let buffer = index.to_buffer()?;
            storage.write_vector_index(collection, &buffer)?;
        }
        Ok(())
    }

    pub fn flush_collection(&self, collection: &str, storage: &Storage) -> Result<(), MastError> {
        if let Some(index) = self.indexes.get(collection) {
            let buffer = index.to_buffer()?;
            storage.write_vector_index(collection, &buffer)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_storage() -> (tempfile::TempDir, Storage) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.mast");
        let storage = Storage::open(&path).unwrap();
        (dir, storage)
    }

    fn random_vec(dims: usize, seed: u64) -> Vec<f32> {
        let mut v = vec![0.0f32; dims];
        let mut h = seed;
        for i in 0..dims {
            h = h.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            v[i] = (h as f32 / u64::MAX as f32) * 2.0 - 1.0;
        }
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        for x in &mut v {
            *x /= norm;
        }
        v
    }

    #[test]
    fn open_empty_storage() {
        let (_dir, storage) = test_storage();
        let mgr = IndexManager::open(&storage).unwrap();
        assert!(mgr.indexes.is_empty());
    }

    #[test]
    fn get_or_create_new() {
        let (_dir, _storage) = test_storage();
        let mut mgr = IndexManager { indexes: HashMap::new() };
        let idx = mgr.get_or_create("test", 32).unwrap();
        assert_eq!(idx.dimensions(), 32);
        assert!(idx.is_empty());
    }

    #[test]
    fn get_or_create_returns_existing() {
        let mut mgr = IndexManager { indexes: HashMap::new() };
        let _ = mgr.get_or_create("test", 32).unwrap();
        // Add a vector to the first one
        mgr.indexes.get("test").unwrap().add(0, &random_vec(32, 0)).unwrap();

        // get_or_create should return the same index
        let idx = mgr.get_or_create("test", 32).unwrap();
        assert_eq!(idx.len(), 1);
    }

    #[test]
    fn flush_and_reload() {
        let (_dir, storage) = test_storage();
        let mut mgr = IndexManager { indexes: HashMap::new() };

        // Create index, add vectors, flush
        {
            let idx = mgr.get_or_create("test_col", 16).unwrap();
            idx.add(0, &random_vec(16, 0)).unwrap();
            idx.add(1, &random_vec(16, 1)).unwrap();
        }
        // We need collection info in storage for reload to work
        // Write a fake collection entry
        {
            let info = crate::types::CollectionInfo {
                name: "test_col".into(),
                dimensions: 16,
                memory_count: 2,
                created_at: 0,
            };
            let encoded = bincode::serialize(&info).unwrap();
            let txn = storage.db.begin_write().unwrap();
            {
                let mut table = txn.open_table(crate::storage::tables::COLLECTIONS).unwrap();
                table.insert("test_col", encoded.as_slice()).unwrap();
            }
            txn.commit().unwrap();
        }

        mgr.flush_all(&storage).unwrap();

        // Reload
        let restored = IndexManager::open(&storage).unwrap();
        let idx = restored.get("test_col").unwrap();
        assert_eq!(idx.len(), 2);
    }

    #[test]
    fn flush_collection_specific() {
        let (_dir, storage) = test_storage();
        let mut mgr = IndexManager { indexes: HashMap::new() };
        let idx = mgr.get_or_create("col_a", 16).unwrap();
        idx.add(0, &random_vec(16, 0)).unwrap();

        mgr.flush_collection("col_a", &storage).unwrap();

        let buf = storage.read_vector_index("col_a").unwrap();
        assert!(buf.is_some());
    }

    #[test]
    fn flush_nonexistent_collection_is_noop() {
        let (_dir, storage) = test_storage();
        let mgr = IndexManager { indexes: HashMap::new() };
        mgr.flush_collection("nope", &storage).unwrap(); // should not error
    }
}
