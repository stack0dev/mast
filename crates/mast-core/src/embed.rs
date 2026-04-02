use crate::error::MastError;
use async_trait::async_trait;

#[async_trait]
pub trait Embedder: Send + Sync {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, MastError>;
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, MastError>;
    fn dimensions(&self) -> usize;
}

/// Deterministic mock embedder for testing. Generates unit vectors from text hashes.
pub struct MockEmbedder {
    dims: usize,
}

impl MockEmbedder {
    pub fn new(dims: usize) -> Self {
        Self { dims }
    }

    fn hash_to_vector(&self, text: &str) -> Vec<f32> {
        let mut vec = vec![0.0f32; self.dims];
        // Simple deterministic hash-based vector generation
        let mut h: u64 = 5381;
        for b in text.bytes() {
            h = h.wrapping_mul(33).wrapping_add(b as u64);
        }
        for i in 0..self.dims {
            // Mix hash with index for each dimension
            let mixed = h.wrapping_mul(i as u64 + 1).wrapping_add(i as u64 * 7919);
            // Map to [-1, 1] range
            vec[i] = ((mixed % 10000) as f32 / 5000.0) - 1.0;
        }
        // Normalize to unit length
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut vec {
                *v /= norm;
            }
        }
        vec
    }
}

#[async_trait]
impl Embedder for MockEmbedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, MastError> {
        Ok(self.hash_to_vector(text))
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, MastError> {
        Ok(texts.iter().map(|t| self.hash_to_vector(t)).collect())
    }

    fn dimensions(&self) -> usize {
        self.dims
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_embedder_dimensions() {
        let emb = MockEmbedder::new(128);
        assert_eq!(emb.dimensions(), 128);
    }

    #[tokio::test]
    async fn mock_embedder_consistent_vectors() {
        let emb = MockEmbedder::new(64);
        let v1 = emb.embed("hello").await.unwrap();
        let v2 = emb.embed("hello").await.unwrap();
        assert_eq!(v1, v2);
    }

    #[tokio::test]
    async fn mock_embedder_different_inputs_different_vectors() {
        let emb = MockEmbedder::new(64);
        let v1 = emb.embed("hello").await.unwrap();
        let v2 = emb.embed("world").await.unwrap();
        assert_ne!(v1, v2);
    }

    #[tokio::test]
    async fn mock_embedder_correct_dimensions() {
        let emb = MockEmbedder::new(256);
        let v = emb.embed("test").await.unwrap();
        assert_eq!(v.len(), 256);
    }

    #[tokio::test]
    async fn mock_embedder_unit_vector() {
        let emb = MockEmbedder::new(64);
        let v = emb.embed("normalize me").await.unwrap();
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }

    #[tokio::test]
    async fn mock_embedder_batch() {
        let emb = MockEmbedder::new(32);
        let texts = vec!["one".to_string(), "two".to_string(), "three".to_string()];
        let results = emb.embed_batch(&texts).await.unwrap();
        assert_eq!(results.len(), 3);
        for v in &results {
            assert_eq!(v.len(), 32);
        }
        // Each should match individual embed
        let v0 = emb.embed("one").await.unwrap();
        assert_eq!(results[0], v0);
    }

    #[tokio::test]
    async fn mock_embedder_empty_batch() {
        let emb = MockEmbedder::new(16);
        let results = emb.embed_batch(&[]).await.unwrap();
        assert!(results.is_empty());
    }
}
