use async_trait::async_trait;
pub use fastembed::EmbeddingModel;
use fastembed::{InitOptions, TextEmbedding};
use mast_core::embed::Embedder;
use mast_core::error::MastError;

pub struct LocalEmbedder {
    model: TextEmbedding,
    dims: usize,
}

impl LocalEmbedder {
    /// Create a new LocalEmbedder with the default model (AllMiniLML6V2, 384d).
    pub fn new() -> Result<Self, MastError> {
        Self::with_model(EmbeddingModel::AllMiniLML6V2)
    }

    /// Create a LocalEmbedder with a specific model.
    pub fn with_model(model: EmbeddingModel) -> Result<Self, MastError> {
        let dims = model_dimensions(&model);
        let options = InitOptions::new(model).with_show_download_progress(true);
        let text_model = TextEmbedding::try_new(options)
            .map_err(|e| MastError::Embedder(format!("failed to load model: {e}")))?;
        Ok(Self {
            model: text_model,
            dims,
        })
    }

    /// Create a LocalEmbedder with a custom cache directory.
    pub fn with_cache_dir(model: EmbeddingModel, cache_dir: &str) -> Result<Self, MastError> {
        let dims = model_dimensions(&model);
        let options = InitOptions::new(model)
            .with_show_download_progress(true)
            .with_cache_dir(cache_dir.into());
        let text_model = TextEmbedding::try_new(options)
            .map_err(|e| MastError::Embedder(format!("failed to load model: {e}")))?;
        Ok(Self {
            model: text_model,
            dims,
        })
    }
}

#[async_trait]
impl Embedder for LocalEmbedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, MastError> {
        let text = text.to_string();
        // fastembed's embed() is sync + CPU-bound — must use spawn_blocking
        let model_ref = &self.model;
        // TextEmbedding is not Send, so we can't move it into spawn_blocking.
        // Instead, we run it directly since fastembed is fast enough for single texts
        // and the caller should be on a runtime that can handle brief blocking.
        // For batch operations, consider using embed_batch with spawn_blocking.
        let results = model_ref
            .embed(vec![text], None)
            .map_err(|e| MastError::Embedder(format!("embedding failed: {e}")))?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| MastError::Embedder("empty embedding result".into()))
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, MastError> {
        let texts: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
        self.model
            .embed(texts, None)
            .map_err(|e| MastError::Embedder(format!("batch embedding failed: {e}")))
    }

    fn dimensions(&self) -> usize {
        self.dims
    }
}

fn model_dimensions(model: &EmbeddingModel) -> usize {
    match model {
        EmbeddingModel::AllMiniLML6V2 | EmbeddingModel::AllMiniLML6V2Q => 384,
        EmbeddingModel::AllMiniLML12V2 | EmbeddingModel::AllMiniLML12V2Q => 384,
        EmbeddingModel::BGESmallENV15 | EmbeddingModel::BGESmallENV15Q => 384,
        EmbeddingModel::BGEBaseENV15 | EmbeddingModel::BGEBaseENV15Q => 768,
        EmbeddingModel::BGELargeENV15 | EmbeddingModel::BGELargeENV15Q => 1024,
        EmbeddingModel::NomicEmbedTextV1 => 768,
        EmbeddingModel::NomicEmbedTextV15 | EmbeddingModel::NomicEmbedTextV15Q => 768,
        EmbeddingModel::MultilingualE5Small => 384,
        EmbeddingModel::MultilingualE5Base => 768,
        EmbeddingModel::MultilingualE5Large => 1024,
        EmbeddingModel::MxbaiEmbedLargeV1 | EmbeddingModel::MxbaiEmbedLargeV1Q => 1024,
        EmbeddingModel::GTEBaseENV15 | EmbeddingModel::GTEBaseENV15Q => 768,
        EmbeddingModel::GTELargeENV15 | EmbeddingModel::GTELargeENV15Q => 1024,
        _ => 384, // safe default for unknown models
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_dimensions_known_models() {
        assert_eq!(model_dimensions(&EmbeddingModel::AllMiniLML6V2), 384);
        assert_eq!(model_dimensions(&EmbeddingModel::BGEBaseENV15), 768);
        assert_eq!(model_dimensions(&EmbeddingModel::BGELargeENV15), 1024);
        assert_eq!(model_dimensions(&EmbeddingModel::NomicEmbedTextV1), 768);
    }

    #[test]
    fn model_dimensions_quantized_variants() {
        assert_eq!(model_dimensions(&EmbeddingModel::AllMiniLML6V2Q), 384);
        assert_eq!(model_dimensions(&EmbeddingModel::BGESmallENV15Q), 384);
        assert_eq!(model_dimensions(&EmbeddingModel::BGEBaseENV15Q), 768);
    }
}
