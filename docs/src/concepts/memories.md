# Memories

A **Memory** is the fundamental unit of data in MAST. It combines content, a vector embedding, metadata, tier classification, and lifecycle tracking into a single record.

## Memory Struct

```rust
pub struct Memory {
    pub id: Ulid,                          // Time-sortable unique ID
    pub collection: String,                // Namespace (e.g., "user:alice")
    pub content: String,                   // The memory text
    pub embedding: Vec<f32>,               // Vector embedding
    pub metadata: HashMap<String, String>, // Arbitrary key-value pairs
    pub tier: Tier,                        // Core | Active | Background | Archive
    pub created_at: i64,                   // Unix timestamp (seconds)
    pub access_count: u64,                 // Number of times recalled
    pub last_accessed: i64,                // Last recall timestamp
    pub ttl: Option<i64>,                  // Time-to-live in seconds (None = no expiry)
    pub compacted_at: Option<i64>,         // When this memory was compacted
    pub parent_ids: Vec<Ulid>,             // Source memories (compaction provenance)
}
```

## Fields in Detail

### id

A ULID (Universally Unique Lexicographically Sortable Identifier). Generated automatically on `store()`. ULIDs are time-sortable, so newer memories have lexicographically greater IDs.

### collection

A namespace string that isolates memories. Each collection has its own vector index, full-text index, and graph. See [Collections](./collections.md).

### content

The raw text of the memory. This is indexed for full-text search and passed to the embedder for vector generation.

### embedding

A dense float vector produced by the embedder. All memories in a collection must share the same dimensionality. You can provide a pre-computed embedding via `StoreRequest.embedding`, or let MAST generate one via the `Embedder` trait.

### metadata

Arbitrary string key-value pairs. Useful for tagging memories with source, session ID, topic, or any domain-specific labels. Metadata is indexed and can be filtered during recall.

```rust
HashMap::from([
    ("source".into(), "conversation".into()),
    ("session".into(), "abc123".into()),
    ("topic".into(), "preferences".into()),
])
```

### tier

One of four tiers that classify a memory's lifecycle stage:

| Tier | Weight | Purpose |
|------|--------|---------|
| `Core` | 1.0 | Foundational knowledge, rarely changes |
| `Active` | 0.9 | Recent, frequently accessed (default) |
| `Background` | 0.7 | Older, less accessed |
| `Archive` | 0.5 | Historical, low relevance |

Tiers affect recall scoring. See [Compaction](./compaction.md) for tier promotion.

### ttl

Optional time-to-live in seconds. A memory expires when `now - created_at >= ttl`. Expired memories are excluded from `recall()` results automatically. Call `vacuum()` to hard-delete expired memories.

### compacted_at

Set when a memory has been compacted into a higher-tier synthesis. Compacted memories are excluded from `recall()` but remain visible in `list()` for provenance tracing.

### parent_ids

Links a compacted memory to its source memories. This preserves provenance -- you can always trace a high-tier insight back to the raw events that produced it.

## StoreRequest

To create a memory, build a `StoreRequest`:

```rust
pub struct StoreRequest {
    pub collection: String,
    pub content: String,
    pub embedding: Option<Vec<f32>>,       // None = auto-embed
    pub metadata: HashMap<String, String>,
    pub tier: Tier,                         // defaults to Active
    pub ttl: Option<i64>,                   // None = no expiry
}
```

If `embedding` is `None`, MAST calls the provided `Embedder` to generate one. If `Some`, the provided vector is used directly.

## Lifecycle

1. **Created** via `store()` -- assigned a ULID, embedded, indexed
2. **Recalled** via `recall()` -- `access_count` incremented, `last_accessed` updated
3. **Compacted** via `compact()` -- marked with `compacted_at`, hidden from recall
4. **Expired** -- excluded from recall when TTL elapsed
5. **Vacuumed** via `vacuum()` -- hard-deleted from storage and indexes
6. **Deleted** via `delete()` -- removed from storage and indexes immediately
