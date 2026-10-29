# MAST — Memory Agent Storage Tiers

> An embeddable storage engine purpose-built for AI agent memory. The SQLite of agent memory.

## What This Is

MAST is a Rust library (with C FFI) that provides a single-file, embeddable storage engine optimized for the unique access patterns of AI agent memory. It is not a general-purpose database. It is not a wrapper around Postgres. It is a purpose-built primitive that combines time-tiered storage, vector search, metadata filtering, and graph relationships into one embeddable package.

**Analogy:** SQLite is to relational data as MAST is to agent memory.

## Why It Exists

Every existing agent memory system (Mem0, Zep, Letta) wraps general-purpose databases — Postgres, Qdrant, Redis — because nothing purpose-built exists. Agent memory has unique requirements that none of these databases are optimized for:

- **Time-tiered compaction as a first-class primitive.** Memories should automatically evolve from raw events → distilled patterns → core identity over time. No existing system does this at the storage level.
- **Compaction as synthesis, not summarization.** Many individual memories become new higher-order insights. "User rejected salesy copy 15 times" compacts into "user hates salesy copy." This is not truncation. It is knowledge creation.
- **Hybrid search in a single query.** Vector similarity + metadata filters + time-tier blending + full-text, without orchestrating across 3 different databases.
- **Graph relationships alongside flat memories.** Entity extraction produces relationships (user → prefers → dark mode) that need to live next to the memories themselves.
- **Embeddable, zero-dependency.** A Python developer should `pip install` a package that bundles MAST. No Docker, no server process, no external database.

## Product Strategy

- **Open-source the engine** (Apache 2.0 or MIT). Build community, become the standard.
- **Monetize via Stack0 Memory** — a managed cloud service running the same engine. Same data format, portable between local and cloud.
- **Playbook:** SQLite/Turso, DuckDB/MotherDuck, Redis/Redis Cloud.

---

## Document Scope and Status

This document intentionally combines long-term product vision with the current implementation.

- **Planned** sections describe the target architecture and APIs.
- **Current** sections describe behavior implemented in the repository today.
- When there is a conflict, **Current** behavior and the addendum below are the source of truth for contributors.

---

## Architecture Overview (Planned)

```
MAST Engine
├── Storage Layer: append-only log per collection, organized by tier
├── Vector Index: HNSW for semantic search (embedded, no external dependency)
├── Metadata Index: inverted index for tags, types, sources, timestamps
├── Full-Text Index: BM25 for keyword search
├── Graph Layer: adjacency lists for entity relationships
├── Compaction Engine: built-in tier promotion with pluggable LLM interface
├── TTL / Expiry: handled at the storage level, not a cleanup job
└── Single file on disk (WAL + data segments)
```

### Data Model

**Status:** Planned target model. For current runtime contracts and fields, see `## Specification Addendum — Current Implementation Contracts (v0)`.

The fundamental unit is a **Memory**:

```rust
pub struct Memory {
    pub id: MemoryId,              // ULID (time-sortable, globally unique)
    pub collection: CollectionId,  // namespace (e.g., "user:abc", "agent:assistant")
    pub tier: Tier,                // Hot | Warm | Cool | Cold
    pub content: String,           // the actual memory text
    pub embedding: Vec<f32>,       // vector embedding
    pub metadata: Metadata,        // arbitrary key-value pairs
    pub source: Source,            // where this memory came from
    pub created_at: Timestamp,     // when the original event occurred
    pub compacted_at: Option<Timestamp>, // when this was last compacted
    pub parent_ids: Vec<MemoryId>, // memories this was compacted from (provenance)
    pub ttl: Option<Duration>,     // optional expiry
    pub access_count: u32,         // how often this has been recalled
    pub last_accessed: Timestamp,  // last recall time
}

pub enum Tier {
    Hot,   // Raw events, conversations, observations. High volume, recent.
    Warm,  // Summarized sessions, extracted facts. Medium volume, days-weeks old.
    Cool,  // Distilled patterns, preferences, behavioral insights. Low volume, weeks-months.
    Cold,  // Core identity, foundational knowledge. Rarely changes.
}

pub struct Edge {
    pub id: EdgeId,
    pub source: EntityId,
    pub target: EntityId,
    pub relation: String,          // "prefers", "works_at", "dislikes"
    pub weight: f32,               // confidence / strength
    pub memory_ids: Vec<MemoryId>, // which memories support this edge
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
```

### Collections

Memories are namespaced by collection. A collection is a logical grouping — typically one per user, one per agent, or one per user-agent pair. Collections are cheap to create and isolated from each other.

```rust
// Examples:
"user:abc123"           // all memories about user abc123
"agent:assistant"       // the assistant's own memories
"user:abc123:assistant" // memories from user abc123's interactions with the assistant
```

---

## Core Operations

**Status:** Planned API surface. Current implemented operations: `store`, `store_batch`, `recall`, `get`, `delete`, `list`, `info`, `compact`, `vacuum`, `relate`, `unrelate`, `traverse`, `subgraph`, `close`.

### STORE — Write + embed + index in one atomic operation

```rust
pub async fn store(&self, req: StoreRequest) -> Result<MemoryId> {
    // 1. Generate ULID
    // 2. Compute embedding (via pluggable embedder)
    // 3. Write to append-only log
    // 4. Update HNSW vector index
    // 5. Update metadata inverted index
    // 6. Update full-text index
    // 7. Return MemoryId
}

pub struct StoreRequest {
    pub collection: CollectionId,
    pub content: String,
    pub metadata: Option<Metadata>,
    pub source: Source,
    pub tier: Option<Tier>,       // defaults to Hot
    pub ttl: Option<Duration>,
    pub embedding: Option<Vec<f32>>, // pre-computed, or MAST computes it
}
```

### RECALL — Vector similarity + metadata filter + tier blending, single query

```rust
pub async fn recall(&self, req: RecallRequest) -> Result<Vec<ScoredMemory>> {
    // 1. Embed the query (if text provided)
    // 2. Vector search across specified tiers
    // 3. Apply metadata filters
    // 4. Apply time decay scoring
    // 5. Blend results across tiers with configurable weights
    // 6. Return ranked results
    // 7. Update access_count and last_accessed on returned memories
}

pub struct RecallRequest {
    pub collection: CollectionId,
    pub query: RecallQuery,        // Text (will be embedded) or Vector
    pub tiers: Vec<Tier>,          // which tiers to search (default: all)
    pub tier_weights: Option<TierWeights>, // how to blend tier results
    pub filters: Option<MetadataFilter>,   // key-value filters
    pub time_range: Option<TimeRange>,     // created_at filter
    pub limit: usize,              // max results (default: 10)
    pub min_score: Option<f32>,    // minimum similarity threshold
}

pub struct TierWeights {
    pub hot: f32,   // default: 1.0
    pub warm: f32,  // default: 0.9
    pub cool: f32,  // default: 0.8
    pub cold: f32,  // default: 0.7
}
```

### COMPACT — Tier promotion as a native operation

```rust
pub async fn compact(&self, req: CompactRequest) -> Result<CompactResult> {
    // 1. Select memories eligible for promotion based on age + access patterns
    // 2. Group related memories (by topic, entity, time window)
    // 3. Call pluggable LLM to synthesize groups into higher-tier memories
    // 4. Write new compacted memories to target tier
    // 5. Mark source memories as compacted (retain for provenance, stop serving)
    // 6. Update indexes
    // 7. Return compaction report
}

pub struct CompactRequest {
    pub collection: CollectionId,
    pub source_tier: Tier,
    pub target_tier: Tier,
    pub strategy: CompactionStrategy,
    pub max_batch_size: usize,     // how many memories to process at once
    pub dry_run: bool,             // preview without writing
}

pub enum CompactionStrategy {
    /// Time-based: compact memories older than threshold
    Age { older_than: Duration },
    /// Access-based: compact memories with low access counts
    AccessDecay { min_age: Duration, max_access_count: u32 },
    /// Hybrid: combine age and access patterns
    Hybrid { age_weight: f32, access_weight: f32 },
    /// Manual: compact specific memory IDs
    Manual { memory_ids: Vec<MemoryId> },
}

/// Pluggable LLM interface for synthesis
pub trait Compactor: Send + Sync {
    /// Given a batch of related memories, produce a synthesized higher-tier memory.
    /// This is NOT summarization. The output should be a new insight that didn't
    /// exist in any single input memory.
    async fn synthesize(&self, memories: &[Memory]) -> Result<CompactedMemory>;
}
```

### RELATE / TRAVERSE — Graph operations

```rust
/// Add or update an entity relationship
pub async fn relate(&self, req: RelateRequest) -> Result<EdgeId>;

/// Traverse the entity graph
pub async fn traverse(&self, req: TraverseRequest) -> Result<Vec<Edge>>;

/// Get all entities and relationships for a collection
pub async fn subgraph(&self, collection: CollectionId) -> Result<Graph>;

pub struct RelateRequest {
    pub collection: CollectionId,
    pub source: EntityId,
    pub target: EntityId,
    pub relation: String,
    pub weight: f32,
    pub memory_ids: Vec<MemoryId>, // supporting memories
}

pub struct TraverseRequest {
    pub collection: CollectionId,
    pub start: EntityId,
    pub max_depth: usize,          // default: 2
    pub relation_filter: Option<String>, // filter by relation type
    pub min_weight: Option<f32>,   // minimum edge weight
}
```

### EXPIRE — TTL handled at the storage level

```rust
/// Memories with TTL are automatically expired.
/// Expired memories are soft-deleted (marked, not removed) until next vacuum.
pub async fn vacuum(&self, collection: CollectionId) -> Result<VacuumResult>;
```

### SNAPSHOT — Point-in-time snapshot of memory state

```rust
/// Create a portable snapshot of a collection's entire memory state.
/// Can be restored to another MAST instance.
pub async fn snapshot(&self, collection: CollectionId) -> Result<Snapshot>;
pub async fn restore(&self, snapshot: Snapshot) -> Result<()>;
```

---

## Storage Engine Internals (Planned)

### File Format

Single file on disk with the following structure:

```
mast.db
├── Header (magic bytes, version, config)
├── WAL (write-ahead log for crash recovery)
├── Data Segments (append-only, one per tier)
│   ├── hot.segment
│   ├── warm.segment
│   ├── cool.segment
│   └── cold.segment
├── Vector Index (HNSW graph, memory-mapped)
├── Metadata Index (inverted index)
├── Full-Text Index (BM25)
├── Graph Index (adjacency lists)
└── Free List (reclaimed space from vacuumed memories)
```

### Write Path

1. Write to WAL (fsync)
2. Write memory to appropriate tier segment
3. Update in-memory indexes (vector, metadata, full-text, graph)
4. Acknowledge write
5. Background: flush indexes to disk periodically

### Read Path

1. Check in-memory index cache
2. Vector search → candidate set
3. Apply metadata filters → filtered set
4. Score with time decay and tier weights → ranked set
5. Return top-k

### Compaction Pipeline

Compaction runs as a background process (or on-demand):

```
Hot memories (age > threshold)
    │
    ▼
Group by: topic similarity + entity overlap + time proximity
    │
    ▼
For each group: call Compactor::synthesize()
    │
    ▼
Write synthesized memory to Warm tier
    │
    ▼
Mark source Hot memories as compacted
    │
    ▼
(Same process promotes Warm → Cool, Cool → Cold)
```

Compaction is **provenance-preserving**: every compacted memory records which source memories it was synthesized from. You can always trace back to the raw events.

---

## Embedding Interface

MAST does not include an embedding model. It provides a pluggable interface:

```rust
pub trait Embedder: Send + Sync {
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
    fn dimensions(&self) -> usize;
}

// Example implementations (provided as optional features):
// - OpenAIEmbedder (calls OpenAI API)
// - OllamaEmbedder (calls local Ollama)
// - PrecomputedEmbedder (pass your own vectors)
```

When `StoreRequest.embedding` is `Some`, MAST skips the embedder and uses the provided vector directly. This allows callers who already have embeddings to avoid redundant computation.

---

## Configuration

```rust
pub struct MastConfig {
    pub path: PathBuf,                    // database file path
    pub embedding_dimensions: usize,      // vector dimensions (e.g., 1536 for OpenAI)
    pub hnsw_m: usize,                    // HNSW max connections (default: 16)
    pub hnsw_ef_construction: usize,      // HNSW build quality (default: 200)
    pub hnsw_ef_search: usize,            // HNSW search quality (default: 100)
    pub wal_sync: WalSync,                // Fsync | Async | None
    pub compaction: CompactionConfig,
    pub max_memory_size: Option<usize>,   // max bytes for in-memory cache
}

pub struct CompactionConfig {
    pub enabled: bool,
    pub hot_to_warm_age: Duration,        // default: 24 hours
    pub warm_to_cool_age: Duration,       // default: 7 days
    pub cool_to_cold_age: Duration,       // default: 30 days
    pub batch_size: usize,                // default: 50
    pub schedule: CompactionSchedule,     // OnWrite | Interval(Duration) | Manual
}
```

---

## Rust Crate Structure

**Status:** Planned target module layout shown first. Current repository layout is crate-based under `crates/`.

```
mast/
├── Cargo.toml
├── src/
│   ├── lib.rs              // Public API surface
│   ├── store.rs            // STORE operation
│   ├── recall.rs           // RECALL operation
│   ├── compact.rs          // COMPACT operation + Compactor trait
│   ├── graph.rs            // RELATE / TRAVERSE operations
│   ├── types.rs            // Memory, Tier, Edge, etc.
│   ├── config.rs           // MastConfig
│   ├── engine/
│   │   ├── mod.rs
│   │   ├── wal.rs          // Write-ahead log
│   │   ├── segment.rs      // Tier data segments
│   │   ├── freelist.rs     // Space reclamation
│   │   └── file.rs         // Single-file format, mmap
│   ├── index/
│   │   ├── mod.rs
│   │   ├── vector.rs       // HNSW index
│   │   ├── metadata.rs     // Inverted index
│   │   ├── fulltext.rs     // BM25 index
│   │   └── graph.rs        // Adjacency list index
│   ├── embed.rs            // Embedder trait + optional impls
│   ├── snapshot.rs         // Snapshot / restore
│   └── ffi.rs              // C FFI bindings
├── mast-py/                // Python bindings (PyO3)
│   ├── Cargo.toml
│   └── src/lib.rs
├── mast-node/              // Node.js bindings (napi-rs)
│   ├── Cargo.toml
│   └── src/lib.rs
└── mast-cli/               // CLI tool
    ├── Cargo.toml
    └── src/main.rs
```

Current repository layout:

```
mast/
├── crates/
│   ├── mast-core/
│   ├── mast-embed-openai/
│   └── mast-cli/
└── docs/
    └── spec.md
```

## Key Rust Dependencies

| Crate | Purpose |
|-------|---------|
| `usearch` or `hnsw_rs` | HNSW vector index |
| `redb` or `fjall` | Embedded key-value storage for indexes |
| `memmap2` | Memory-mapped file I/O |
| `serde` + `rmp-serde` | Serialization (MessagePack for compact binary format) |
| `ulid` | Time-sortable unique IDs |
| `tantivy` (optional) | Full-text search (or build minimal BM25) |
| `pyo3` | Python bindings |
| `napi-rs` | Node.js bindings |
| `tokio` | Async runtime (for embedder calls, background compaction) |

---

## Usage Example

```rust
use mast::{Mast, MastConfig, StoreRequest, RecallRequest, Tier};

// Open or create a MAST database
let db = Mast::open(MastConfig {
    path: "agent_memory.mast".into(),
    embedding_dimensions: 1536,
    ..Default::default()
})?;

// Set the embedder
db.set_embedder(OpenAIEmbedder::new("sk-..."))?;

// Store a memory
let id = db.store(StoreRequest {
    collection: "user:alice".into(),
    content: "Alice said she prefers dark mode and hates notifications".into(),
    metadata: metadata!{ "source": "conversation", "session": "abc123" },
    ..Default::default()
}).await?;

// Recall relevant memories
let memories = db.recall(RecallRequest {
    collection: "user:alice".into(),
    query: "What are Alice's UI preferences?".into(),
    limit: 5,
    ..Default::default()
}).await?;

// Run compaction (or let it run automatically in background)
let report = db.compact(CompactRequest {
    collection: "user:alice".into(),
    source_tier: Tier::Hot,
    target_tier: Tier::Warm,
    strategy: CompactionStrategy::Age { older_than: Duration::hours(24) },
    ..Default::default()
}).await?;
```

### Python Usage

```python
import mast

db = mast.open("agent_memory.mast", dimensions=1536)
db.set_embedder(mast.OpenAIEmbedder("sk-..."))

# Store
memory_id = db.store(
    collection="user:alice",
    content="Alice said she prefers dark mode",
    metadata={"source": "conversation"}
)

# Recall
results = db.recall(
    collection="user:alice",
    query="What are Alice's UI preferences?",
    limit=5
)
```

---

## Specification Addendum — Current Implementation Contracts (v0)

This section reflects behavior implemented today. It separates **Current Guarantee** from **Planned Contract** to avoid over-promising.

### 1) Durability and Flush Semantics

- **Current Guarantee**
  - `store()` persists memory records and metadata index entries transactionally before returning success.
  - Vector index is in-memory during runtime and persisted on `close()` via index flush.
- **Current Limitation**
  - If process exits uncleanly before `close()`, recent vector index updates may not be persisted.
- **Planned Contract**
  - Add startup rebuild/checkpoint so unflushed vector updates can be recovered deterministically.

### 2) Input Resolution and Validation

- **Current Guarantee**
  - `recall()` requires at least one of `query` or `query_embedding`.
  - If both are provided, `query_embedding` is used.
  - Empty embeddings are rejected for both `store()` and `recall()`.
  - Dimension mismatch is rejected against collection index dimensions.
- **Current Limitation**
  - Empty collection names and empty metadata keys are not explicitly rejected.
  - No explicit max limits for content/metadata sizes are enforced.
- **Planned Contract**
  - Add explicit validation limits and empty-field guards.

### 3) Collection and Dimension Invariants

- **Current Guarantee**
  - Collection isolation is enforced for store/recall/get.
  - First write defines effective vector dimension for a collection index.
  - Subsequent dimension mismatches are rejected.
  - ULID↔seq mappings are maintained for written records.
- **Current Limitation**
  - No explicit integrity-check API for mapping drift.
- **Planned Contract**
  - Add optional integrity scan/repair command.

### 4) Recall Semantics

- **Current Guarantee**
  - `limit=0` falls back to configured `default_limit`.
  - Results are sorted descending by score.
  - Score computed as `1.0 - distance`.
- **Current Limitation**
  - Tie-breaking order is not explicitly specified.
  - No explicit min/max score contract is documented in spec text.
- **Planned Contract**
  - Define deterministic tie-break rules and score range statement.

### 5) Metadata Filters

- **Current Guarantee**
  - `Eq`, `In`, `And`, `Or` are supported, including nesting.
  - `In([])`, `And([])`, and `Or([])` return empty result sets.
- **Current Limitation**
  - No query planner/optimization guarantees for large filter sets.
- **Planned Contract**
  - Add complexity/performance guidance by cardinality.

### 6) Concurrency and Process Model

- **Current Guarantee**
  - Safe operation in the intended single-process embedded usage path.
- **Current Limitation**
  - Multi-process concurrency contract is not explicitly specified.
- **Planned Contract**
  - Document supported/unsupported multi-process access with explicit failure behavior.

### 7) Versioning and Migration

- **Current Guarantee**
  - Database open/create behavior is implemented through underlying storage.
- **Current Limitation**
  - No explicit file-format compatibility policy section in the spec.
- **Planned Contract**
  - Add compatibility matrix and migration policy.

### 8) Error Taxonomy

- **Current Guarantee**
  - Errors are surfaced as typed categories (validation/input, storage, index, embedder).
- **Current Limitation**
  - No stable external error-code table is documented for FFI/client SDK users.
- **Planned Contract**
  - Publish stable error codes and retryability guidance.

### 9) Operational Limits and SLOs

- **Current Guarantee**
  - No formal SLO commitments.
- **Current Limitation**
  - Performance and scale limits are not yet normative.
- **Planned Contract**
  - Add benchmark-backed guidance for dimensions, collection sizes, and target latencies.

### 10) Versioned Serialization

- **Current Guarantee**
  - Memory records use a versioned envelope: byte `0x01` prefix = v1 (current), no recognized prefix = v0 (legacy).
  - v0 data (written before Phase 2) is auto-migrated on read: `ttl=None`, `compacted_at=None`, `parent_ids=[]`.
  - Edge records use the same envelope. v0 edges (written before validity intervals) read as `valid_from=None`, `valid_to=None`.
  - No scan-and-rewrite migration is required when upgrading.
  - JSON snapshots without edge `valid_from`/`valid_to` fields restore with both set to `None`.

### 11) Recall Scoring

- **Current Guarantee**
  - `score = relevance * tier_weight * time_decay_factor * compacted_factor * anchor_boost`
  - Vector relevance: `similarity = 1.0 - distance` (cosine distance from usearch). Full-text relevance: BM25.
  - Hybrid relevance: `vector_weight * vector_score + text_weight * (bm25_score / best_bm25_score)`, where each part already includes the tier, decay and compacted factors. Vector similarity is on a fixed scale and is not rescaled; BM25 is divided by the best BM25 score among the candidates. Chosen over min-max and reciprocal rank fusion on the LOCOMO dev split (see `packages/mast-bench/README.md`).
  - `tier_weight` sourced from `RecallRequest.tier_weights` or `MastConfig.default_tier_weights`. Defaults: Core=1.0, Active=0.9, Background=0.7, Archive=0.5.
  - `time_decay_factor = 0.5^(age_secs / half_life_secs)`, with age measured from `RecallRequest.reference_time` (default: now). If no decay configured, factor is 1.0.
  - `compacted_factor` is 1.0, or `MastConfig.compacted_source_weight` (default 0.5) for memories with `compacted_at` set. A weight of 0.0 or less removes compacted memories from recall.
  - `anchor_boost` is described in §19.
  - Memories with expired TTL (`now - created_at >= ttl`, wall-clock now) are excluded from recall results.
  - `access_count` incremented and `last_accessed` updated on all returned memories after recall.

### 12) Batch Store

- **Current Guarantee**
  - `store_batch()` accepts a `BatchStoreRequest` with multiple items.
  - Items without pre-computed embeddings are embedded via `embed_batch()`.
  - All-or-nothing: if any item fails storage or indexing, all committed items are rolled back.
  - Validates that `embed_batch` returns exactly as many vectors as requested.

### 13) Compaction

- **Current Guarantee**
  - `compact()` selects memories from `source_tier`, filters by strategy, synthesizes via pluggable `Compactor` trait, stores result at `target_tier` with `parent_ids` linking to sources.
  - Source memories are marked with `compacted_at` timestamp. Recall still returns them, scaled by `compacted_source_weight` (§11), so a poor synthesis does not hide the original text.
  - `dry_run=true` returns candidates without writing.
  - `max_batch_size` must be > 0.
  - Compensating rollback: if index insertion fails after storage commit, the new memory is deleted from storage.
  - Strategies: `Age { older_than_secs }`, `AccessDecay`, `Hybrid`, `Manual { ids }`.
- **Current Limitation**
  - `AccessDecay` and `Hybrid` strategies are defined but not yet implemented (treated as select-all).
  - No background scheduler; compaction is manual only.

### 14) Graph Layer

- **Current Guarantee**
  - Edges carry an optional validity interval `[valid_from, valid_to)` in unix seconds. `None` means unbounded on that side.
  - `relate()` creates or upserts an edge. The upsert key is (source, target, relation, valid_from): a fact that changed over time is one edge per interval. `valid_to <= valid_from` is rejected.
  - `invalidate()` sets `valid_to` (default: now) on the open edges for (source, target, relation) and returns how many it closed. It fails when no edge is open or when `ended_at <= valid_from`.
  - `unrelate()` removes every edge for (source, target, relation), across all intervals.
  - `traverse()` performs BFS from a start entity, respecting `max_depth`, `relation_filter`, `min_weight`, and `as_of`. With `as_of` set, only edges valid at that time are followed; without it, all edges are.
  - `subgraph()` returns all edges in a collection.
  - Edges stored in redb with forward and reverse index tables for O(1) entity lookups.
- **Current Limitation**
  - No weighted path scoring or shortest-path algorithms.

### 15) TTL and Vacuum

- **Current Guarantee**
  - `Memory.ttl` is an optional duration in seconds. `None` = no expiry.
  - Expired condition: `now - created_at >= ttl`. `created_at` is the time passed to `store()` (§19) when one was given, so an imported memory with a TTL can expire on arrival.
  - Expired memories are excluded from `recall()` results automatically.
  - `vacuum()` scans a collection and hard-deletes all expired memories (storage + index).
  - Returns `VacuumResult { collection, expired_count }`.
- **Current Limitation**
  - No automatic/scheduled vacuum; must be called explicitly.

### 16) Delete and List

- **Current Guarantee**
  - `delete()` removes a memory from storage (data + metadata index) and vector index.
  - `list()` returns all memories in a collection (including compacted ones).
- **Current Limitation**
  - `list()` performs a full table scan. No pagination or cursor support.

### 17) Atomicity and Rollback

- **Current Guarantee**
  - `store()`: if `index.add()` fails after storage commit, the committed memory is deleted (compensating rollback).
  - `store_batch()`: if any `index.add()` fails, all previously indexed items in the batch are removed from the index, and all stored memories are deleted.
  - `compact()`: if `index.add()` fails for the new compacted memory, it is deleted from storage; source memories are not marked as compacted.
- **Current Limitation**
  - Rollback is best-effort. If the compensating delete itself fails, the orphaned record remains. No crash-recovery reconciliation exists yet.

### 18) Drop and Flush

- **Current Guarantee**
  - `impl Drop for Mast` calls `index_manager.flush_all()`, persisting all in-memory vector indexes to redb.
  - Flush errors are logged via `tracing::error!` but do not panic.
  - `close()` is available for explicit flush with error propagation.
- **Current Limitation**
  - If the process crashes before `Drop` runs, unflushed vector index updates are lost.

### 19) Event Time and Temporal Recall

- **Current Guarantee**
  - `StoreRequest.created_at` and `BatchStoreItem.created_at` set when the memory happened (unix seconds). `None` = now. Time decay, TTL and the anchor boost all use it.
  - `RecallRequest.reference_time` is when the question is asked. `None` = now.
  - `RecallRequest.time_anchor` names a target time and window. When it is `None`, recall parses a relative phrase from `query` (or `text_query`) against `reference_time`: "yesterday", "today", "recently", "lately", "last week|month|year", and "<n> day(s)|week(s)|month(s)|year(s) ago".
  - With an anchor, `anchor_boost = 1 + 0.4 * max(0, 1 - |created_at - target| / window)`. Recall then ranks at least 50 candidates (or `5 * limit`) before cutting to `limit`, so a memory just outside the top `limit` can move into it.
  - Without an anchor, `anchor_boost` is 1.0 and no extra candidates are fetched.
- **Current Limitation**
  - English phrases only. Absolute dates in the query ("in May 2023") are not parsed.

---

## Implementation Phases

This checklist tracks planned roadmap work and is not a literal reflection of current shipped code in the repository.

### Phase 1: Core Storage (Ship this first)
- [x] Single-file format (redb, single-file ACID)
- [x] Memory struct and ULID generation
- [x] STORE operation (write + index)
- [x] RECALL operation (vector search + metadata filter)
- [x] Basic HNSW vector index (via `usearch`)
- [x] Metadata inverted index
- [x] Collection namespacing
- [x] Embedder trait + OpenAI implementation
- [x] Basic CLI (`mast store`, `mast recall`, `mast info`)
- [ ] Publish `mast` crate to crates.io

### Phase 2: Compaction + Graph
- [x] Compaction engine with Compactor trait
- [x] Tier promotion pipeline (manual, via `compact()`)
- [x] Provenance tracking (parent_ids)
- [ ] Background compaction scheduler
- [x] Graph layer (relate, unrelate, traverse, subgraph)
- [ ] Entity extraction as a Compactor plugin
- [x] TTL / expiry + vacuum
- [x] Batch store
- [x] Recall scoring (tier weights, time decay)
- [x] Versioned serialization (v0 compat + v1 envelope)
- [x] Delete + list operations
- [x] Drop impl (flush indexes on drop)
- [x] Compensating rollback on index failures
- [x] CLI: delete, list, vacuum, compact, relate, traverse

### Phase 3: Bindings + Polish
- [ ] Python bindings (PyO3) → publish to PyPI
- [ ] Node.js bindings (napi-rs) → publish to npm
- [ ] C FFI header generation
- [ ] Snapshot / restore
- [ ] Full-text search (BM25)
- [ ] Benchmarks and performance tuning
- [ ] Documentation site

### Phase 4: Stack0 Memory Integration
- [ ] Replace Postgres backend in Stack0 Memory with MAST
- [ ] MAST Cloud: managed MAST instances
- [ ] Replication and backup for cloud tier
- [ ] Multi-tenant isolation

---

## What MAST Is Not

- **Not a general-purpose database.** No SQL. No arbitrary queries. It does one thing.
- **Not a vector database.** It has vector search, but it's not Pinecone or Qdrant. The vector index is one component of a larger memory system.
- **Not a framework.** It doesn't tell you how to build agents. It stores and recalls their memories.
- **Not an LLM.** It has a pluggable LLM interface for compaction, but ships no model.

## What MAST Is

- An embeddable storage engine.
- Purpose-built for agent memory access patterns.
- Time-tiered with automatic knowledge evolution.
- A single file on disk, no server process required.
- Open source. Use it anywhere.

---

*MAST — Memory Agent Storage Tiers*
*The memory engine for AI agents.*