# mast

A memory storage engine for AI applications. Single-file ACID database with vector similarity search, BM25 full-text search, knowledge graphs, and pluggable embedders.

For architecture and roadmap details, see `docs/spec.md`.

## Quick start

```bash
cargo build

# Store a memory with a pre-computed embedding
mast store --db my.mast \
  --collection "user:alice" \
  --content "Alice prefers dark mode" \
  --embedding 0.1,0.2,0.3,0.4 \
  --metadata theme=dark --metadata source=preferences

# Recall by vector similarity
mast recall --db my.mast \
  --collection "user:alice" \
  --query-vec 0.1,0.2,0.3,0.4 \
  --limit 5

# With OpenAI embeddings (requires OPENAI_API_KEY)
mast store --db my.mast \
  --collection "user:alice" \
  --content "Alice prefers dark mode"

mast recall --db my.mast \
  --collection "user:alice" \
  --query "what does alice like"

# With local on-device embeddings (no API key needed)
mast store --db my.mast \
  --collection "user:alice" \
  --content "Alice prefers dark mode" \
  --local

mast recall --db my.mast \
  --collection "user:alice" \
  --query "what does alice like" \
  --local

# Full-text search
mast recall --db my.mast \
  --collection "user:alice" \
  --text-query "dark mode" \
  --search-mode fulltext

# Hybrid search (vector + full-text)
mast recall --db my.mast \
  --collection "user:alice" \
  --query "preferences" \
  --text-query "dark mode" \
  --search-mode hybrid

# Knowledge graph
mast relate --db my.mast \
  --collection "user:alice" \
  --source "alice" --target "dark_mode" --relation "prefers"

mast traverse --db my.mast \
  --collection "user:alice" \
  --start "alice" --max-depth 2

# Snapshot and restore
mast snapshot --db my.mast --collection "user:alice" --output backup.jsonl
mast restore --db my.mast --input backup.jsonl --merge

# Inspect the database
mast info --db my.mast
```

### CLI commands

`store`, `recall`, `info`, `delete`, `list`, `vacuum`, `compact`, `snapshot`, `restore`, `relate`, `traverse`

## Library usage

```rust
use mast_core::{Mast, config::MastConfig, types::*, embed::MockEmbedder};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = MastConfig::default().with_db_path("my.mast");
    let mut mast = Mast::open(config)?;
    let embedder = MockEmbedder::new(4);

    // Store
    let memory = mast.store(StoreRequest {
        collection: "notes".into(),
        content: "The quick brown fox".into(),
        embedding: Some(vec![0.1, 0.9, 0.0, 0.0]),
        metadata: HashMap::from([("source".into(), "test".into())]),
        tier: Tier::Active,
        ttl: None,
    }, &embedder).await?;

    // Recall
    let results = mast.recall(RecallRequest {
        collection: "notes".into(),
        query: None,
        query_embedding: Some(vec![0.1, 0.9, 0.0, 0.0]),
        limit: 5,
        filter: Some(MetadataFilter::Eq("source".into(), "test".into())),
    }, &embedder).await?;

    for r in &results {
        println!("[{:.4}] {}", r.score, r.memory.content);
    }

    // Direct lookup
    let mem = mast.get("notes", &memory.id)?;

    // Collection stats
    let info = mast.info()?;

    mast.close()?;
    Ok(())
}
```

## Architecture

```
mast/
  crates/
    mast-core/           Zero vendor deps. The engine.
    mast-embed-openai/   OpenAI embedder
    mast-embed-voyage/   Voyage AI embedder
    mast-embed-local/    Local on-device embedder (fastembed)
    mast-cli/            CLI binary
    mast-ffi/            C FFI bindings
    mast-py/             Python bindings (PyO3/maturin)
    mast-node/           Node.js bindings (napi-rs)
  packages/
    mast-agent/          Agent loop + memory-first context engine
    mast-channels/       Channel adapters (CLI, HTTP/WS, Telegram, Slack)
    mast-tools/          MCP client + built-in tools
    mast-bench/          LOCOMO benchmark harness
    create-mast-agent/   npx scaffold + wizard
  plugins/
    openclaw/            OpenClaw memory plugin
  apps/
    Soma/                macOS SwiftUI therapeutic journal app
  docs/                  mdbook documentation
  scripts/               Build scripts (e.g. build-mast.sh for FFI)
```

**Storage:** [redb](https://github.com/cberner/redb) -- single-file, ACID, crash-safe. All data lives in one `.mast` file.

**Vector index:** [usearch](https://github.com/unum-cloud/usearch) -- HNSW graph with cosine similarity. Per-collection indexes, serialized into redb.

**Full-text index:** BM25 with Porter stemming. Inverted posting lists stored in redb. Supports standalone full-text search or hybrid (weighted blend with vector results via min-max normalization).

**IDs:** ULIDs -- time-sortable, globally unique. Mapped to `u64` sequential keys for usearch internally.

**Serialization:** bincode -- fast binary format for internal storage.

### Data model

Each **Memory** has:

| Field | Type | Description |
|-------|------|-------------|
| `id` | ULID | Unique identifier |
| `collection` | String | Namespace (e.g. `"user:alice"`) |
| `content` | String | The text content |
| `embedding` | Vec\<f32\> | Vector representation |
| `metadata` | HashMap\<String, String\> | Arbitrary key-value pairs |
| `tier` | Tier | `Core`, `Active`, `Background`, or `Archive` |
| `created_at` | i64 | Unix timestamp |
| `access_count` | u64 | Number of times recalled |
| `last_accessed` | i64 | Last recall timestamp |
| `ttl` | Option\<i64\> | Time-to-live in seconds (auto-expired on recall) |
| `compacted_at` | Option\<i64\> | Timestamp if this memory was compacted |
| `parent_ids` | Vec\<ULID\> | Source memory IDs if created by compaction |

Collections are isolated -- recall in one collection never returns results from another.

### Search modes

```rust
SearchMode::Vector                                // cosine similarity (default)
SearchMode::FullText                              // BM25 with Porter stemming
SearchMode::Hybrid { vector_weight, text_weight } // weighted blend
```

### Knowledge graph

Memories can be linked with typed, weighted edges. BFS traversal and subgraph extraction built in.

```rust
mast.add_edge("collection", "alice", "bob", "knows", 1.0)?;
let nodes = mast.traverse("collection", "alice", max_depth, None)?;
```

### Metadata filtering

Filters are applied during vector search (not post-filter), using usearch's `filtered_search`:

```rust
MetadataFilter::Eq("env", "prod")
MetadataFilter::In("color", vec!["red", "blue"])
MetadataFilter::And(vec![...])
MetadataFilter::Or(vec![...])
```

Backed by an inverted index in redb: `(collection, key, value) -> [memory_ids]`.

### Compaction

Memories can be compacted across tiers. A pluggable `Compactor` trait summarizes batches of memories into a single compacted memory, preserving lineage via `parent_ids`.

### Snapshot / restore

Export collections to JSONL (memories + edges). Restore with `--merge` to skip duplicates.

### Embedder trait

The core engine has no vendor dependencies. Embedders are pluggable:

```rust
#[async_trait]
pub trait Embedder: Send + Sync {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, MastError>;
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, MastError>;
    fn dimensions(&self) -> usize;
}
```

Included implementations:
- `MockEmbedder` -- deterministic hash-based vectors for testing
- `OpenAIEmbedder` -- calls OpenAI `/v1/embeddings` API
- `VoyageEmbedder` -- calls Voyage AI `/v1/embeddings` API
- `LocalEmbedder` -- on-device via [fastembed](https://github.com/Anush008/fastembed-rs) (AllMiniLML6V2, 384d)

## Agent framework

TypeScript monorepo in `packages/`. Build memory-first AI agents with multiple LLM providers.

```bash
npx create-mast-agent my-agent
```

- **mast-agent** -- Dual-engine agent: Anthropic (via Claude Agent SDK) or custom loop (OpenAI, OpenRouter). TOML config.
- **mast-channels** -- CLI REPL, HTTP/WebSocket server, Telegram, Slack adapters.
- **mast-tools** -- MCP client, tool registry, built-in memory tools (`remember`, `search_memory`, `forget`).
- **create-mast-agent** -- `npx` scaffold with interactive wizard.

## Bindings

| Language | Crate | Notes |
|----------|-------|-------|
| C | `mast-ffi` | Opaque handle, function pointer vtable for embedder/compactor |
| Python | `mast-py` | PyO3/maturin, async via `pyo3-async-runtimes` |
| Node.js | `mast-node` | napi-rs, `AsyncTask` pattern, optional `local-embedder` feature |
| Swift | via `mast-ffi` | `MastBridge` actor in Soma app, SPM package |

## Plugins

### OpenClaw Memory Plugin

Drop-in long-term memory for [OpenClaw](https://github.com/openclaw) via `mast-node` napi bindings. Vector + full-text hybrid search with knowledge graphs.

```bash
cd crates/mast-node && napi build --release
cd plugins/openclaw && npm install && npm run build
```

6 tools (`mast_store`, `mast_search`, `mast_forget`, `mast_relate`, `mast_traverse`, `mast_graph`) + auto-recall/capture hooks. See [`plugins/openclaw/SKILL.md`](plugins/openclaw/SKILL.md).

## Tests

```bash
cargo test
```

220+ tests: unit, integration, embedder, CLI, and FFI. No API keys needed -- all tests use `MockEmbedder` with pre-computed embeddings.

## License

MIT
