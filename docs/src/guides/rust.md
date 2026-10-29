# Rust Guide

Complete guide to using MAST from Rust.

## Dependencies

```toml
[dependencies]
mast-core = { path = "crates/mast-core" }
mast-embed-openai = { path = "crates/mast-embed-openai" }  # optional
tokio = { version = "1", features = ["full"] }
```

## Configuration

```rust
use mast_core::config::MastConfig;

let config = MastConfig::default()
    .with_db_path("/path/to/agent.mast")
    .with_default_collection("default")
    .with_default_limit(10)
    .with_default_dimensions(1536)
    .with_default_tier_weights(TierWeights {
        core: 1.0,
        active: 0.9,
        background: 0.7,
        archive: 0.5,
    })
    .with_default_time_decay(TimeDecay { half_life_secs: 86400 });
```

All fields have sensible defaults. The only field you typically need to set is `db_path`.

## Opening and Closing

```rust
use mast_core::Mast;

let mut mast = Mast::open(config)?;

// ... use mast ...

mast.close()?;  // explicit flush, propagates errors
// Or let Drop handle it (logs errors but does not panic)
```

## Storing Memories

### Single Store

```rust
use mast_core::types::*;
use std::collections::HashMap;

let memory = mast.store(
    StoreRequest {
        collection: "notes".into(),
        content: "Rust's ownership system prevents data races".into(),
        embedding: None,
        metadata: HashMap::from([("topic".into(), "rust".into())]),
        tier: Tier::Active,
        ttl: None,
        created_at: None,
    },
    &embedder,
).await?;
```

### Batch Store

```rust
let memories = mast.store_batch(
    BatchStoreRequest {
        collection: "notes".into(),
        items: vec![
            BatchStoreItem {
                content: "Item one".into(),
                embedding: None,
                metadata: HashMap::new(),
                tier: Tier::Active,
                ttl: None,
                created_at: None,
            },
            BatchStoreItem {
                content: "Item two".into(),
                embedding: None,
                metadata: HashMap::new(),
                tier: Tier::Active,
                ttl: Some(3600),
                created_at: None,
            },
        ],
    },
    &embedder,
).await?;
```

Batch store uses `embed_batch()` for efficiency and is atomic -- if any item fails, all are rolled back.

### Pre-Computed Embeddings

```rust
let memory = mast.store(
    StoreRequest {
        collection: "notes".into(),
        content: "pre-embedded text".into(),
        embedding: Some(vec![0.1, 0.2, 0.3, /* ... */]),
        metadata: HashMap::new(),
        tier: Tier::Active,
        ttl: None,
        created_at: None,
    },
    &embedder,  // not called
).await?;
```

## Recalling Memories

### Vector Search

```rust
let results = mast.recall(
    RecallRequest {
        collection: "notes".into(),
        query: Some("ownership and borrowing".into()),
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
```

### Full-Text Search

```rust
let results = mast.recall(
    RecallRequest {
        collection: "notes".into(),
        query: None,
        query_embedding: None,
        text_query: Some("ownership data races".into()),
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

### Hybrid Search

```rust
let results = mast.recall(
    RecallRequest {
        collection: "notes".into(),
        query: Some("ownership".into()),
        query_embedding: None,
        text_query: Some("ownership".into()),
        search_mode: SearchMode::Hybrid {
            vector_weight: 0.6,
            text_weight: 0.4,
        },
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

### With Metadata Filter

```rust
let results = mast.recall(
    RecallRequest {
        collection: "notes".into(),
        query: Some("systems programming".into()),
        filter: Some(MetadataFilter::And(vec![
            MetadataFilter::Eq("topic".into(), "rust".into()),
            MetadataFilter::In("level".into(), vec!["advanced".into(), "expert".into()]),
        ])),
        ..Default::default()
    },
    &embedder,
).await?;
```

## CRUD Operations

```rust
// Get by ID
let memory = mast.get("notes", &memory_id)?;

// Delete by ID
mast.delete("notes", &memory_id)?;

// List all memories in a collection
let all = mast.list("notes")?;

// List all collections
let collections = mast.info()?;
```

## Graph Operations

```rust
// Create or update an edge
let edge_id = mast.relate(RelateRequest {
    collection: "notes".into(),
    source: "rust".into(),
    target: "memory_safety".into(),
    relation: "provides".into(),
    weight: 0.95,
    memory_ids: vec![],
    valid_from: None,
    valid_to: None,
})?;

// Remove an edge
mast.unrelate("notes", "rust", "memory_safety", "provides")?;

// BFS traversal
let edges = mast.traverse(TraverseRequest {
    collection: "notes".into(),
    start: "rust".into(),
    max_depth: 3,
    relation_filter: None,
    min_weight: Some(0.5),
    as_of: None,
})?;

// Get all edges in a collection
let graph = mast.subgraph("notes")?;
```

## Compaction

```rust
use mast_core::compact::Compactor;

let result = mast.compact(
    CompactRequest {
        collection: "notes".into(),
        source_tier: Tier::Active,
        target_tier: Tier::Background,
        strategy: CompactionStrategy::Age { older_than_secs: 86400 },
        max_batch_size: 10,
        dry_run: false,
    },
    &compactor,   // implements Compactor trait
    &embedder,
).await?;

println!("Compacted {} into {} new memories",
    result.compacted_count, result.new_memory_ids.len());
```

## TTL and Vacuum

```rust
// Store with TTL (expires after 1 hour)
mast.store(StoreRequest {
    ttl: Some(3600),
    ..
}, &embedder).await?;

// Remove expired memories
let result = mast.vacuum("notes")?;
println!("Removed {} expired memories", result.expired_count);
```

## Snapshots

```rust
// Export
let mut file = std::fs::File::create("backup.jsonl")?;
let result = mast.snapshot("notes", &mut file)?;

// Import (collection must not already exist)
let file = std::fs::File::open("backup.jsonl")?;
let mut reader = std::io::BufReader::new(file);
let result = mast.restore(&mut reader)?;
```

## Error Handling

All operations return `Result<T, MastError>`:

```rust
pub enum MastError {
    Storage(String),       // redb errors
    Index(String),         // usearch errors
    Embedder(String),      // embedding failures
    NotFound(String),      // entity not found
    InvalidInput(String),  // validation errors
    Serialization(String), // bincode/serde errors
    Compaction(String),    // compaction failures
    Graph(String),         // graph operation errors
    Snapshot(String),      // snapshot/restore errors
}
```
