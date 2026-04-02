# Embeddings

MAST does not include an embedding model. Instead, it provides a pluggable `Embedder` trait that you implement or use a provided implementation.

## The Embedder Trait

```rust
#[async_trait]
pub trait Embedder: Send + Sync {
    /// Embed a single text into a vector.
    async fn embed(&self, text: &str) -> Result<Vec<f32>, MastError>;

    /// Embed multiple texts in a batch (for efficiency).
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, MastError>;

    /// Return the vector dimensionality.
    fn dimensions(&self) -> usize;
}
```

The `Embedder` is passed to `store()` and `recall()` as a reference. MAST calls it when:

- `store()` is called without a pre-computed embedding (`StoreRequest.embedding = None`)
- `recall()` is called with a text query (`RecallRequest.query = Some(...)`) and no `query_embedding`
- `store_batch()` needs to embed items without pre-computed embeddings
- `compact()` needs to embed the synthesized content

## OpenAI Embedder

The `mast-embed-openai` crate provides an OpenAI-compatible embedder:

```rust
use mast_embed_openai::OpenAIEmbedder;

let embedder = OpenAIEmbedder::new("sk-your-api-key".into())
    .with_model("text-embedding-3-small")   // default
    .with_dimensions(1536)                   // default
    .with_base_url("https://api.openai.com"); // default
```

### Builder Methods

| Method | Default | Description |
|--------|---------|-------------|
| `with_model(model)` | `text-embedding-3-small` | OpenAI model name |
| `with_dimensions(dims)` | `1536` | Output vector dimensions |
| `with_base_url(url)` | `https://api.openai.com` | API base URL (for proxies or compatible APIs) |

### Compatible APIs

The `with_base_url()` method lets you point to any OpenAI-compatible API:

```rust
// Azure OpenAI
let embedder = OpenAIEmbedder::new(api_key)
    .with_base_url("https://your-resource.openai.azure.com");

// Local proxy or compatible service
let embedder = OpenAIEmbedder::new(api_key)
    .with_base_url("http://localhost:8080");
```

## Pre-Computed Embeddings

If you already have embeddings, skip the embedder entirely:

```rust
let memory = mast.store(
    StoreRequest {
        collection: "notes".into(),
        content: "some text".into(),
        embedding: Some(vec![0.1, 0.2, 0.3, /* ... */]),
        metadata: HashMap::new(),
        tier: Tier::Active,
        ttl: None,
    },
    &embedder,  // not called when embedding is Some
).await?;
```

For recall with a pre-computed query vector:

```rust
let results = mast.recall(
    RecallRequest {
        collection: "notes".into(),
        query: None,
        query_embedding: Some(vec![0.1, 0.2, 0.3, /* ... */]),
        text_query: None,
        search_mode: SearchMode::Vector,
        limit: 10,
        filter: None,
        tier_weights: None,
        time_decay: None,
    },
    &embedder,  // not called when query_embedding is Some
).await?;
```

## Writing a Custom Embedder

Implement the `Embedder` trait for any embedding provider:

```rust
use async_trait::async_trait;
use mast_core::embed::Embedder;
use mast_core::error::MastError;

struct MyEmbedder { /* ... */ }

#[async_trait]
impl Embedder for MyEmbedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, MastError> {
        // Call your embedding service
        let vector = my_api_call(text).await
            .map_err(|e| MastError::Embedder(e.to_string()))?;
        Ok(vector)
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, MastError> {
        // Batch embed for efficiency, or fall back to sequential
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed(text).await?);
        }
        Ok(results)
    }

    fn dimensions(&self) -> usize {
        768
    }
}
```

## MockEmbedder (Testing)

MAST includes a `MockEmbedder` for testing that generates deterministic unit vectors from text hashes:

```rust
use mast_core::embed::MockEmbedder;

let embedder = MockEmbedder::new(128); // 128 dimensions
// Same text always produces the same vector
// Different text produces different vectors
```
