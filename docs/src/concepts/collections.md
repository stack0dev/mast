# Collections

Collections are namespaces that isolate memories, indexes, and graph edges from each other. Every memory belongs to exactly one collection.

## What a Collection Provides

Each collection maintains its own:

- **Vector index** (usearch HNSW) with a fixed dimensionality
- **Full-text index** (BM25 posting lists)
- **Metadata index** (inverted index)
- **Graph edges** (forward and reverse indexes)
- **Memory count and stats**

## Creating Collections

Collections are created implicitly on first write. There is no explicit "create collection" operation. The first `store()` call to a new collection name creates it and sets its vector dimensions based on the first embedding.

```rust
// This creates the "user:alice" collection with 1536 dimensions
mast.store(
    StoreRequest {
        collection: "user:alice".into(),
        content: "Alice likes dark mode".into(),
        embedding: None,  // embedder produces 1536-dim vector
        ..Default::default()
    },
    &embedder,
).await?;
```

## Dimension Enforcement

The first memory stored in a collection defines its vector dimensions. All subsequent memories must have embeddings with the same dimensionality. Mismatches are rejected with an error:

```
embedding dimensions 768 do not match collection dimensions 1536
```

## Collection Info

Use `info()` to list all collections and their stats:

```rust
pub struct CollectionInfo {
    pub name: String,        // collection name
    pub dimensions: usize,   // vector dimensionality
    pub memory_count: u64,   // number of stored memories
    pub created_at: i64,     // creation timestamp
}
```

```rust
let collections = mast.info()?;
for col in &collections {
    println!("{}: {} memories, {} dims", col.name, col.memory_count, col.dimensions);
}
```

## Naming Conventions

Collection names are arbitrary strings. Common patterns:

```
"user:alice"               // all memories about a user
"agent:assistant"          // the agent's own memories
"user:alice:assistant"     // session-scoped memories
"project:acme"             // project-scoped memories
```

## Isolation

Collections are fully isolated:

- A `recall()` on collection A never returns results from collection B.
- `vacuum()` only affects the specified collection.
- `snapshot()` exports a single collection.
- Graph edges are scoped to a collection.

## Operations by Collection

| Operation | Scope |
|-----------|-------|
| `store()` | Single collection |
| `recall()` | Single collection |
| `get()` | Single collection |
| `delete()` | Single collection |
| `list()` | Single collection |
| `vacuum()` | Single collection |
| `compact()` | Single collection |
| `relate()` | Single collection |
| `traverse()` | Single collection |
| `subgraph()` | Single collection |
| `snapshot()` | Single collection |
| `info()` | All collections |
