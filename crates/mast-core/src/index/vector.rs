use crate::error::MastError;

pub struct VectorIndex {
    index: usearch::Index,
    dimensions: usize,
}

impl VectorIndex {
    pub fn new(dimensions: usize) -> Result<Self, MastError> {
        let opts = usearch::IndexOptions {
            dimensions,
            metric: usearch::MetricKind::Cos,
            quantization: usearch::ScalarKind::F32,
            ..Default::default()
        };
        let index = usearch::Index::new(&opts)
            .map_err(|e| MastError::Index(format!("failed to create index: {e}")))?;
        index
            .reserve(1024)
            .map_err(|e| MastError::Index(format!("failed to reserve capacity: {e}")))?;
        Ok(Self { index, dimensions })
    }

    pub fn from_buffer(buffer: &[u8], dimensions: usize) -> Result<Self, MastError> {
        let opts = usearch::IndexOptions {
            dimensions,
            metric: usearch::MetricKind::Cos,
            quantization: usearch::ScalarKind::F32,
            ..Default::default()
        };
        let index = usearch::Index::new(&opts)
            .map_err(|e| MastError::Index(format!("failed to create index for loading: {e}")))?;

        // usearch Rust bindings: load from file or view from buffer
        // We'll use a tempfile approach for loading since the Rust API may not have load_from_buffer
        index
            .load_from_buffer(buffer)
            .map_err(|e| MastError::Index(format!("failed to load index from buffer: {e}")))?;

        // Reserve extra capacity so new vectors can be added after loading
        let current = index.size();
        index
            .reserve(current + 1024)
            .map_err(|e| MastError::Index(format!("failed to reserve capacity: {e}")))?;

        Ok(Self { index, dimensions })
    }

    pub fn reserve_additional(&self, count: usize) -> Result<(), MastError> {
        let needed = self.index.size() + count;
        if needed > self.index.capacity() {
            self.index
                .reserve(needed)
                .map_err(|e| MastError::Index(format!("failed to reserve capacity: {e}")))?;
        }
        Ok(())
    }

    pub fn add(&self, key: u64, vector: &[f32]) -> Result<(), MastError> {
        self.index
            .add(key, vector)
            .map_err(|e| MastError::Index(format!("failed to add vector: {e}")))?;
        Ok(())
    }

    pub fn remove(&self, key: u64) -> Result<(), MastError> {
        self.index
            .remove(key)
            .map_err(|e| MastError::Index(format!("failed to remove vector: {e}")))?;
        Ok(())
    }

    pub fn search(&self, query: &[f32], limit: usize) -> Result<Vec<(u64, f32)>, MastError> {
        let results = self
            .index
            .search(query, limit)
            .map_err(|e| MastError::Index(format!("search failed: {e}")))?;

        Ok(results
            .keys
            .into_iter()
            .zip(results.distances.into_iter())
            .collect())
    }

    pub fn filtered_search(
        &self,
        query: &[f32],
        limit: usize,
        predicate: impl Fn(u64) -> bool,
    ) -> Result<Vec<(u64, f32)>, MastError> {
        let results = self
            .index
            .filtered_search(query, limit, predicate)
            .map_err(|e| MastError::Index(format!("filtered search failed: {e}")))?;

        Ok(results
            .keys
            .into_iter()
            .zip(results.distances.into_iter())
            .collect())
    }

    pub fn to_buffer(&self) -> Result<Vec<u8>, MastError> {
        let len = self.index.serialized_length();
        let mut buffer = vec![0u8; len];
        self.index
            .save_to_buffer(&mut buffer)
            .map_err(|e| MastError::Index(format!("failed to save index to buffer: {e}")))?;
        Ok(buffer)
    }

    pub fn len(&self) -> usize {
        self.index.size()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn dimensions(&self) -> usize {
        self.dimensions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn create_new_index() {
        let idx = VectorIndex::new(64).unwrap();
        assert_eq!(idx.dimensions(), 64);
        assert_eq!(idx.len(), 0);
        assert!(idx.is_empty());
    }

    #[test]
    fn add_and_search() {
        let idx = VectorIndex::new(32).unwrap();
        let v1 = random_vec(32, 1);
        let v2 = random_vec(32, 2);
        let v3 = random_vec(32, 3);

        idx.add(0, &v1).unwrap();
        idx.add(1, &v2).unwrap();
        idx.add(2, &v3).unwrap();
        assert_eq!(idx.len(), 3);

        // Search for v1 should return v1 as closest
        let results = idx.search(&v1, 3).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].0, 0); // key 0 should be closest to itself
    }

    #[test]
    fn filtered_search_respects_predicate() {
        let idx = VectorIndex::new(32).unwrap();
        for i in 0..10u64 {
            idx.add(i, &random_vec(32, i)).unwrap();
        }

        // Only allow even keys
        let results = idx
            .filtered_search(&random_vec(32, 0), 10, |key| key % 2 == 0)
            .unwrap();
        for (key, _dist) in &results {
            assert_eq!(key % 2, 0, "filtered_search returned odd key {key}");
        }
    }

    #[test]
    fn remove_then_search() {
        let idx = VectorIndex::new(32).unwrap();
        let v1 = random_vec(32, 1);
        let v2 = random_vec(32, 2);

        idx.add(0, &v1).unwrap();
        idx.add(1, &v2).unwrap();
        assert_eq!(idx.len(), 2);

        idx.remove(0).unwrap();

        let results = idx.search(&v1, 10).unwrap();
        // Key 0 should not appear
        for (key, _) in &results {
            assert_ne!(*key, 0);
        }
    }

    #[test]
    fn buffer_roundtrip() {
        let idx = VectorIndex::new(16).unwrap();
        for i in 0..5u64 {
            idx.add(i, &random_vec(16, i)).unwrap();
        }

        let buffer = idx.to_buffer().unwrap();
        assert!(!buffer.is_empty());

        let restored = VectorIndex::from_buffer(&buffer, 16).unwrap();
        assert_eq!(restored.len(), 5);

        // Search should still work
        let results = restored.search(&random_vec(16, 0), 5).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].0, 0);
    }

    #[test]
    fn empty_index_search() {
        let idx = VectorIndex::new(16).unwrap();
        let results = idx.search(&random_vec(16, 42), 5).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn filtered_search_rejects_all() {
        let idx = VectorIndex::new(16).unwrap();
        for i in 0..5u64 {
            idx.add(i, &random_vec(16, i)).unwrap();
        }

        let results = idx
            .filtered_search(&random_vec(16, 0), 5, |_| false)
            .unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn search_limit_larger_than_index() {
        let idx = VectorIndex::new(16).unwrap();
        idx.add(0, &random_vec(16, 0)).unwrap();
        idx.add(1, &random_vec(16, 1)).unwrap();

        let results = idx.search(&random_vec(16, 0), 100).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn add_duplicate_key_rejected() {
        let idx = VectorIndex::new(16).unwrap();
        let v1 = random_vec(16, 1);
        let v2 = random_vec(16, 2);

        idx.add(0, &v1).unwrap();
        let result = idx.add(0, &v2);
        assert!(result.is_err());
    }

    #[test]
    fn buffer_roundtrip_empty_index() {
        let idx = VectorIndex::new(8).unwrap();
        let buffer = idx.to_buffer().unwrap();
        let restored = VectorIndex::from_buffer(&buffer, 8).unwrap();
        assert_eq!(restored.len(), 0);
        assert!(restored.is_empty());
    }
}
