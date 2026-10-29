# Rust API Reference

Core API surface of the `Mast` struct in `mast-core`.

## Mast

```rust
pub struct Mast { /* private */ }
```

### Constructor

#### `Mast::open(config: MastConfig) -> Result<Self, MastError>`

Opens or creates a MAST database at the configured path. Creates the file and all internal tables if they do not exist.

### Storage Operations

#### `store(&mut self, request: StoreRequest, embedder: &dyn Embedder) -> Result<Memory, MastError>`

Stores a single memory. Generates an embedding if `request.embedding` is `None`. Sets `created_at` from `request.created_at`, or now when it is `None`. Indexes the memory for vector search, full-text search, and metadata filtering. Returns the created `Memory`.

**Errors:**
- `InvalidInput` if embedding is empty or dimensions mismatch
- `Embedder` if embedding generation fails
- `Index` if vector index insertion fails (storage is rolled back)

#### `store_batch(&mut self, request: BatchStoreRequest, embedder: &dyn Embedder) -> Result<Vec<Memory>, MastError>`

Stores multiple memories atomically. Uses `embed_batch()` for items without pre-computed embeddings. All-or-nothing: if any item fails, all are rolled back.

**Errors:**
- `Embedder` if `embed_batch` returns wrong number of vectors
- `InvalidInput` if any embedding is empty or dimensions mismatch
- `Index` if any vector index insertion fails (all rolled back)

### Search Operations

#### `recall(&self, request: RecallRequest, embedder: &dyn Embedder) -> Result<Vec<RecallResult>, MastError>`

Searches for memories matching the query. Supports Vector, FullText, and Hybrid search modes. Updates `access_count` and `last_accessed` on all returned memories.

**Behavior:**
- `limit: 0` falls back to `config.default_limit`
- Compacted memories score `config.compacted_source_weight` (default 0.5) times as much; a weight of 0.0 or less excludes them
- Excludes expired memories (`now - created_at >= ttl`)
- Time decay measures age from `request.reference_time` (default: now)
- A relative time phrase in the query ("two weeks ago"), or `request.time_anchor`, boosts memories created near that time by up to 40%
- Results sorted descending by score

### CRUD Operations

#### `get(&self, collection: &str, id: &Ulid) -> Result<Option<Memory>, MastError>`

Retrieves a single memory by collection and ID. Returns `None` if not found.

#### `delete(&mut self, collection: &str, id: &Ulid) -> Result<(), MastError>`

Deletes a memory from storage, vector index, full-text index, and metadata index.

#### `list(&self, collection: &str) -> Result<Vec<Memory>, MastError>`

Lists all memories in a collection, including compacted ones. Full table scan, no pagination.

#### `info(&self) -> Result<Vec<CollectionInfo>, MastError>`

Returns metadata for all collections.

### Compaction

#### `compact(&mut self, request: CompactRequest, compactor: &dyn Compactor, embedder: &dyn Embedder) -> Result<CompactResult, MastError>`

Compacts memories from `source_tier` to `target_tier` using the provided strategy and compactor.

**Errors:**
- `InvalidInput` if `max_batch_size` is 0
- `Compaction` if synthesis fails
- `Index` if indexing the synthesized memory fails (storage rolled back)

### TTL

#### `vacuum(&mut self, collection: &str) -> Result<VacuumResult, MastError>`

Removes all expired memories from a collection. A memory is expired when `now - created_at >= ttl`.

### Graph

#### `relate(&self, request: RelateRequest) -> Result<Ulid, MastError>`

Creates or updates an edge. If an edge with the same (source, target, relation, valid_from) exists, updates weight, memory_ids and valid_to (upsert). Returns the edge ID. `InvalidInput` when `valid_to <= valid_from`.

#### `invalidate(&self, request: InvalidateRequest) -> Result<usize, MastError>`

Sets `valid_to` (default: now) on the open edges for (source, target, relation). Returns how many it closed. `NotFound` if no edge is open; `InvalidInput` if `ended_at <= valid_from`.

#### `unrelate(&self, collection: &str, source: &str, target: &str, relation: &str) -> Result<(), MastError>`

Removes every edge for (source, target, relation), across all validity intervals. Returns `NotFound` if none exists.

#### `traverse(&self, request: TraverseRequest) -> Result<Vec<Edge>, MastError>`

BFS traversal from a starting entity. Returns all edges encountered, respecting `max_depth`, `relation_filter`, `min_weight`, and `as_of` (only edges valid at that time). Cycle-safe (visits each entity at most once).

#### `subgraph(&self, collection: &str) -> Result<Graph, MastError>`

Returns all edges in a collection.

### Snapshots

#### `snapshot(&self, collection: &str, writer: &mut dyn Write) -> Result<SnapshotResult, MastError>`

Exports a collection to JSONL format. Writes header, then memories, then edges.

#### `restore(&mut self, reader: &mut dyn BufRead) -> Result<RestoreResult, MastError>`

Imports a collection from JSONL format. The collection must not already exist. Indexes all imported memories and edges. Invalid lines produce warnings but do not abort.

### Lifecycle

#### `close(&mut self) -> Result<(), MastError>`

Flushes all vector indexes to storage. Call before dropping for explicit error handling.

Also implements `Drop`, which calls flush (errors logged, not propagated).

---

## MastConfig

```rust
pub struct MastConfig {
    pub db_path: PathBuf,
    pub default_collection: String,    // default: "default"
    pub default_limit: usize,          // default: 10
    pub default_dimensions: usize,     // default: 1536
    pub default_tier_weights: TierWeights,
    pub default_time_decay: Option<TimeDecay>,
    pub durability: Durability,
    pub compacted_source_weight: f32,  // default: 0.5; 0.0 removes compacted memories from recall
}
```

Builder methods: `with_db_path()`, `with_default_collection()`, `with_default_limit()`, `with_default_dimensions()`, `with_default_tier_weights()`, `with_default_time_decay()`, `with_durability()`, `with_compacted_source_weight()`.

---

## Key Types

See [Memories](../concepts/memories.md) and [Search](../concepts/search.md) for detailed type documentation.

| Type | Description |
|------|-------------|
| `Memory` | Core data unit |
| `StoreRequest` | Input for `store()` |
| `BatchStoreRequest` | Input for `store_batch()` |
| `RecallRequest` | Input for `recall()` |
| `RecallResult` | Output from `recall()` (memory + score) |
| `CompactRequest` | Input for `compact()` |
| `CompactResult` | Output from `compact()` |
| `RelateRequest` | Input for `relate()` |
| `InvalidateRequest` | Input for `invalidate()` |
| `TraverseRequest` | Input for `traverse()` |
| `TimeAnchor` | Target time and window for the recall time boost |
| `Edge` | Graph edge |
| `Graph` | Collection of edges |
| `CollectionInfo` | Collection metadata |
| `MetadataFilter` | Eq, In, And, Or filter tree |
| `TierWeights` | Per-tier scoring weights |
| `TimeDecay` | Half-life decay configuration |
| `SearchMode` | Vector, FullText, or Hybrid |
| `Tier` | Core, Active, Background, Archive |
| `CompactionStrategy` | Age, AccessDecay, Hybrid, Manual |
| `VacuumResult` | Output from `vacuum()` |
| `SnapshotResult` | Output from `snapshot()` |
| `RestoreResult` | Output from `restore()` |

---

## Traits

### Embedder

```rust
#[async_trait]
pub trait Embedder: Send + Sync {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, MastError>;
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, MastError>;
    fn dimensions(&self) -> usize;
}
```

### Compactor

```rust
#[async_trait]
pub trait Compactor: Send + Sync {
    async fn synthesize(&self, memories: &[Memory]) -> Result<CompactedMemory, MastError>;
}
```

---

## MastError

```rust
pub enum MastError {
    Storage(String),
    Index(String),
    Embedder(String),
    NotFound(String),
    InvalidInput(String),
    Serialization(String),
    Compaction(String),
    Graph(String),
    Snapshot(String),
}
```

Implements `std::error::Error`, `Display`, `Debug`, `Send`, `Sync`.
