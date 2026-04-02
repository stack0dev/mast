# Changelog

## Phase 1: Core Storage

The foundation: single-file storage, vector search, and basic operations.

- Single-file database using redb (ACID, copy-on-write B-tree)
- `Memory` struct with ULID, collection, content, embedding, metadata, tier
- `store()` operation: write + embed + index atomically
- `recall()` operation: vector search (cosine similarity via usearch HNSW)
- Metadata inverted index with `Eq`, `In`, `And`, `Or` filters
- Collection namespacing with per-collection vector indexes
- `Embedder` trait for pluggable embedding providers
- `OpenAIEmbedder` implementation (text-embedding-3-small)
- `MastConfig` with builder pattern
- `MastError` typed error enum
- `get()`, `delete()` CRUD operations
- `info()` for collection listing
- CLI: `store`, `recall`, `info` commands
- `MockEmbedder` for deterministic testing

## Phase 2: Compaction, Graph, TTL

Tier-based memory evolution, entity relationships, and lifecycle management.

### Compaction
- `Compactor` trait with `synthesize()` method
- `CompactRequest` with strategies: Age, AccessDecay, Hybrid, Manual
- `CompactResult` with source/compacted counts and provenance tracking
- `parent_ids` field on Memory for compaction provenance
- `compacted_at` field: compacted memories excluded from recall
- Dry run mode for previewing candidates
- `max_batch_size` validation (must be > 0)
- `MockCompactor` for testing (concatenates content)

### Graph Layer
- `Edge` struct: source, target, relation, weight, memory_ids
- `relate()`: create or upsert edges
- `unrelate()`: remove edges by (source, target, relation)
- `traverse()`: BFS traversal with depth, relation, and weight filters
- `subgraph()`: return all edges in a collection
- Forward and reverse edge index tables for O(1) lookups

### TTL and Lifecycle
- `ttl` field on Memory (optional seconds-based expiry)
- Expired memories excluded from recall (`now - created_at >= ttl`)
- `vacuum()`: hard-delete expired memories from storage and indexes
- `list()`: return all memories including compacted ones

### Scoring
- Recall score: `similarity * tier_weight * time_decay_factor`
- Configurable `TierWeights` (Core=1.0, Active=0.9, Background=0.7, Archive=0.5)
- Configurable `TimeDecay` with half-life
- Access stats: `access_count` and `last_accessed` updated on recall

### Storage
- Versioned serialization envelope (v0 compat + v1 with version byte prefix)
- V0 auto-migration on read (ttl=None, compacted_at=None, parent_ids=[])
- `store_batch()` with atomic rollback
- Compensating rollback on index insertion failures (store, store_batch, compact)

### Infrastructure
- `Drop` impl: flush vector indexes on drop
- `close()` for explicit flush with error propagation
- CLI: `delete`, `list`, `vacuum`, `compact`, `relate`, `traverse` commands

## Phase 3: Full-Text, Snapshots, Bindings, FFI

Keyword search, data portability, and multi-language support.

### Full-Text Search (BM25)
- Built-in BM25 implementation with redb-backed posting lists
- Tokenizer: lowercase, split on non-alphanumeric, remove short tokens, stop words
- `SearchMode::FullText` for keyword search
- `SearchMode::Hybrid` with min-max normalized score blending
- `RecallRequest.text_query` field for full-text query
- Per-collection full-text stats (doc count, total tokens)

### Snapshots
- JSONL format: header + memory lines + edge lines
- `snapshot()`: export a collection to any `Write` stream
- `restore()`: import from any `BufRead` stream
- Version validation ("1.0")
- Collection existence check on restore
- Warning-tolerant: invalid lines produce warnings, do not abort
- Vector index flush after restore

### C FFI (`mast-ffi`)
- `MastHandle` opaque type
- `MastEmbedderVtable` callback struct (embed, embed_batch, dimensions)
- `MastFfiError` with error codes
- Functions: open, close, store, recall, get, delete, info, vacuum
- Functions: relate, unrelate, snapshot, restore
- Memory-safe free functions for all result types
- `MastMetadata` struct for key-value pair passing

### Python Bindings (`mast-py`)
- PyO3 bindings via maturin
- `Mast` class with all operations
- Python embedder protocol (duck-typed `embed()` method)
- `PyMemory`, `PyRecallResult`, `PyCollectionInfo`, `PyEdge` classes
- `PySnapshotResult`, `PyRestoreResult` classes
- Optional `embed_batch` with sequential fallback
- Metadata filter as `dict[str, str]` with AND semantics

### Node.js Bindings (`mast-node`)
- napi-rs bindings
- `MastDb` class with all operations
- Embedding via `ThreadsafeFunction<String>`
- `JsMemory`, `JsRecallResult`, `JsCollectionInfo`, `JsEdge` types
- `JsSnapshotResult`, `JsRestoreResult` types

### CLI Enhancements
- `snapshot` and `restore` commands
- Full-text and hybrid search modes in `recall`
- `--text-query` and `--search-mode` flags

### Test Infrastructure
- 183 tests: 82 core unit + 61 integration + 16 embedder + 24 CLI
- `MockEmbedder`: deterministic hash-based vectors
- `MockCompactor`: concatenation-based synthesis
- `tempfile::tempdir()` for isolated test databases
- wiremock-based HTTP tests for OpenAI embedder
