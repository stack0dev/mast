# Introduction

MAST (Memory Agent Storage Tiers) is an embeddable storage engine purpose-built for AI agent memory. Think of it as "the SQLite of agent memory" -- a single-file, zero-server database that combines vector search, full-text search, metadata filtering, graph relationships, and time-tiered compaction into one package.

## Why MAST Exists

Every existing agent memory system wraps general-purpose databases -- Postgres, Qdrant, Redis -- because nothing purpose-built exists. Agent memory has unique requirements:

- **Time-tiered compaction as a first-class primitive.** Memories evolve from raw events to distilled patterns to core identity. This should happen at the storage level.
- **Compaction as synthesis, not summarization.** "User rejected salesy copy 15 times" compacts into "user hates salesy copy." This is knowledge creation, not truncation.
- **Hybrid search in a single query.** Vector similarity + metadata filters + full-text + tier blending, without orchestrating across multiple databases.
- **Graph relationships alongside flat memories.** Entity relationships (user -> prefers -> dark mode) live next to the memories themselves.
- **Embeddable, zero-dependency.** `pip install` a package, no Docker, no server.

## Key Features

| Feature | Description |
|---------|-------------|
| **Single-file storage** | redb-backed ACID storage in one file |
| **Vector search** | Cosine similarity via usearch HNSW index |
| **Full-text search** | Built-in BM25 with tokenizer and stop words |
| **Hybrid search** | Weighted sum of vector similarity and BM25 scaled to the best match |
| **Metadata filtering** | Eq, In, And, Or filters with nesting |
| **Memory tiers** | Core, Active, Background, Archive |
| **Compaction** | Pluggable synthesis via `Compactor` trait |
| **Graph layer** | Weighted edges, BFS traversal, entity relations |
| **TTL and expiry** | Per-memory TTL with vacuum cleanup |
| **Snapshots** | JSONL export/import for portability |
| **Pluggable embeddings** | `Embedder` trait -- bring your own model |

## Language Support

| Language | Crate / Package | Binding |
|----------|----------------|---------|
| Rust | `mast-core` | Native |
| Python | `mast-memory` | PyO3 via maturin |
| Node.js | `mast-memory` | napi-rs |
| C/C++ | `mast-ffi` | C FFI with callback vtable |
| CLI | `mast-cli` | Command-line interface |

## Architecture at a Glance

```
MAST Engine
+-- Storage Layer: redb (single-file ACID, key-value)
+-- Vector Index: usearch HNSW (per-collection, cosine metric)
+-- Full-Text Index: BM25 (per-collection, redb-backed posting lists)
+-- Metadata Index: inverted index (redb-backed posting lists)
+-- Graph Layer: forward/reverse edge indexes for entity relationships
+-- Compaction Engine: pluggable Compactor trait for tier promotion
+-- Serialization: bincode (storage) + serde_json (snapshots)
```

## What MAST Is Not

- **Not a general-purpose database.** No SQL. No arbitrary queries.
- **Not a vector database.** Vector search is one component of a larger memory system.
- **Not a framework.** It does not tell you how to build agents.
- **Not an LLM.** It has a pluggable LLM interface for compaction, but ships no model.
