# Performance

Guidelines for optimizing MAST performance.

## Embedding Dimensions

The choice of embedding dimensions directly impacts memory usage, index size, and search speed.

| Dimensions | Model Example | Index Memory per Vector | Use Case |
|------------|---------------|------------------------|----------|
| 256 | Custom | ~1 KB | Prototyping, small datasets |
| 768 | many open-source models | ~3 KB | Balanced quality/performance |
| 1536 | OpenAI text-embedding-3-small | ~6 KB | Production, high quality |
| 3072 | OpenAI text-embedding-3-large | ~12 KB | Maximum quality |

Lower dimensions = faster search + less memory. Use the smallest dimension that meets your quality requirements.

## Batch Operations

Use `store_batch()` instead of repeated `store()` calls:

- `embed_batch()` is called once instead of N individual embed calls
- Reduces round trips to embedding APIs
- Single transaction for all writes

```rust
// Prefer this:
mast.store_batch(BatchStoreRequest {
    collection: "notes".into(),
    items: items,  // vec of BatchStoreItem
}, &embedder).await?;

// Over this:
for item in items {
    mast.store(StoreRequest { .. }, &embedder).await?;
}
```

## Search Tuning

### Overfetch Factor

MAST internally fetches `limit * 3 + 10` candidates from the vector/full-text index, then filters and re-ranks. This means:

- Setting `limit: 5` actually searches for ~25 candidates
- This handles filtering out compacted/expired memories without missing results
- Higher limits incur proportionally more index search cost

### Search Mode Selection

| Mode | Best For | Cost |
|------|----------|------|
| `Vector` | Semantic similarity | One embedding + HNSW search |
| `FullText` | Exact keyword matching | No embedding needed, posting list lookups |
| `Hybrid` | Best overall relevance | Both vector + full-text, then merge |

Full-text search is faster than vector search because it does not require an embedding call.

### Metadata Filters

Metadata filters run after the initial vector/full-text search. For vector search with metadata filters, MAST uses `filtered_search` on the HNSW index, which pre-filters candidates. This is efficient for selective filters but may be slower for very broad filters.

## Memory Management

### Vector Index Caching

Vector indexes are held in memory during the lifetime of the `Mast` instance. The index is persisted to redb on `close()` or `Drop`. Keeping the instance open avoids repeated index deserialization.

### Index Capacity

usearch pre-allocates capacity in blocks of 1024 vectors. After loading from a buffer, it reserves `current_size + 1024` additional slots. This avoids frequent reallocations for typical workloads.

## Storage

### redb Characteristics

- Single-file ACID database
- Copy-on-write B-tree
- No write-ahead log to manage separately
- Good for read-heavy workloads
- Single-writer, multiple-reader

### Data Size Estimates

Per memory, storage overhead is approximately:

| Component | Size |
|-----------|------|
| Memory record (bincode) | ~content_len + dims*4 + metadata + 200 bytes overhead |
| Vector index entry | ~dims * 4 bytes + HNSW graph edges |
| Full-text posting entries | ~tokens * 30 bytes |
| Metadata index entries | ~metadata_pairs * 40 bytes |
| ULID/seq mappings | ~64 bytes |

## Vacuum and Compaction

### Vacuum

Run `vacuum()` periodically on collections with TTL-bearing memories. Vacuuming removes expired memories from storage and indexes, reclaiming space.

### Compaction

Compaction reduces the number of active memories by synthesizing them into higher-tier summaries. This improves recall speed (fewer candidates to search) and reduces storage size.

**Recommended compaction strategy:**

1. Start with `CompactionStrategy::Age { older_than_secs: 86400 }` (1 day)
2. Set `max_batch_size` to 5-20 memories (larger batches produce better synthesis but consume more LLM tokens)
3. Run as a periodic background job

## Close and Flush

Always call `close()` or let `Drop` run. If the process exits before either happens, vector index updates since the last flush are lost (storage data is safe due to redb's ACID properties).

```rust
// Explicit close with error handling
mast.close()?;

// Or rely on Drop (logs errors but does not panic)
drop(mast);
```

## CLI Performance Tips

When using the CLI for batch operations, keep the database handle open as long as possible. Each CLI invocation opens and closes the database, which includes index serialization/deserialization.

For bulk operations, prefer the Rust API or language bindings over repeated CLI calls.
