# Quick Start

Get MAST running in 5 minutes with Rust.

## Add Dependencies

```toml
[dependencies]
mast-core = { path = "crates/mast-core" }
mast-embed-openai = { path = "crates/mast-embed-openai" }  # optional
tokio = { version = "1", features = ["full"] }
```

## Open a Database

```rust
use mast_core::config::MastConfig;
use mast_core::Mast;

let config = MastConfig::default()
    .with_db_path("my_agent.mast")
    .with_default_dimensions(1536);

let mut mast = Mast::open(config)?;
```

## Set Up an Embedder

Using OpenAI:

```rust
use mast_embed_openai::OpenAIEmbedder;

let embedder = OpenAIEmbedder::new(std::env::var("OPENAI_API_KEY")?)
    .with_model("text-embedding-3-small")
    .with_dimensions(1536);
```

Or bring your own by implementing the `Embedder` trait (see [Embeddings](./concepts/embeddings.md)).

## Store a Memory

```rust
use mast_core::types::*;
use std::collections::HashMap;

let memory = mast.store(
    StoreRequest {
        collection: "user:alice".into(),
        content: "Alice prefers dark mode and dislikes notifications".into(),
        embedding: None,  // auto-generated via embedder
        metadata: HashMap::from([
            ("source".into(), "conversation".into()),
        ]),
        tier: Tier::Active,
        ttl: None,
        created_at: None,
    },
    &embedder,
).await?;

println!("Stored: {}", memory.id);
```

## Recall Memories

```rust
let results = mast.recall(
    RecallRequest {
        collection: "user:alice".into(),
        query: Some("What are Alice's UI preferences?".into()),
        query_embedding: None,
        text_query: None,
        search_mode: SearchMode::Vector,
        limit: 5,
        filter: None,
        tier_weights: None,
        time_decay: None,
        reference_time: None,
        time_anchor: None,
    },
    &embedder,
).await?;

for result in &results {
    println!("[{:.4}] {}", result.score, result.memory.content);
}
```

## Full-Text Search

```rust
let results = mast.recall(
    RecallRequest {
        collection: "user:alice".into(),
        query: None,
        query_embedding: None,
        text_query: Some("dark mode".into()),
        search_mode: SearchMode::FullText,
        limit: 5,
        filter: None,
        tier_weights: None,
        time_decay: None,
        reference_time: None,
        time_anchor: None,
    },
    &embedder,
).await?;
```

## Create a Graph Relationship

```rust
use mast_core::types::RelateRequest;

mast.relate(RelateRequest {
    collection: "user:alice".into(),
    source: "alice".into(),
    target: "dark_mode".into(),
    relation: "prefers".into(),
    weight: 0.9,
    memory_ids: vec![memory.id],
    valid_from: None,
    valid_to: None,
})?;
```

## Close the Database

```rust
mast.close()?;
```

The database also flushes indexes automatically on `Drop`, but calling `close()` explicitly lets you handle errors.

## Next Steps

- [Memories](./concepts/memories.md) -- understand the data model
- [Search](./concepts/search.md) -- vector, full-text, and hybrid search
- [Compaction](./concepts/compaction.md) -- tier-based memory synthesis
- [Rust Guide](./guides/rust.md) -- complete Rust usage guide
