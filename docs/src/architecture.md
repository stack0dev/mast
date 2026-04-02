# Architecture

MAST's internal architecture consists of three layers: storage, indexing, and serialization.

## Workspace Layout

```
crates/
+-- mast-core/         Core engine (storage, index, types, config, graph, compaction, snapshot)
+-- mast-embed-openai/ OpenAI embedder implementation
+-- mast-cli/          Command-line interface
+-- mast-ffi/          C FFI bindings
+-- mast-py/           Python bindings (PyO3)
+-- mast-node/         Node.js bindings (napi-rs)
```

`mast-core` has zero vendor dependencies for embeddings. The `Embedder` trait is the boundary -- all embedding logic lives outside the core.

## Storage Layer (redb)

All persistent data lives in a single redb database file. redb is an embedded key-value store with ACID transactions, copy-on-write B-trees, and a single-writer/multiple-reader concurrency model.

### Tables

| Table | Key | Value | Purpose |
|-------|-----|-------|---------|
| `memories` | `(collection, ulid_bytes)` | Versioned-serialized Memory | Primary data store |
| `collections` | `collection_name` | Serialized CollectionInfo | Collection metadata |
| `seq` | `collection_name` | `u64` | Next sequential key for usearch |
| `ulid_to_seq` | `(collection, ulid_bytes)` | `u64` | ULID to sequential key mapping |
| `seq_to_ulid` | `(collection, u64_be_bytes)` | `ulid_bytes` | Sequential key to ULID mapping |
| `meta_index` | `(collection, key, value)` | Serialized `Vec<[u8; 16]>` | Metadata inverted index |
| `vector_index` | `collection_name` | Serialized usearch buffer | Persisted vector index |
| `edges` | `(collection, edge_id)` | Serialized Edge | Graph edge store |
| `edge_index_fwd` | `(collection, source, relation)` | Serialized `Vec<[u8; 16]>` | Forward edge index |
| `edge_index_rev` | `(collection, target, relation)` | Serialized `Vec<[u8; 16]>` | Reverse edge index |
| `ft_posting` | `(collection, term)` | Serialized PostingList | Full-text posting lists |
| `ft_stats` | `collection_name` | Serialized `(doc_count, total_tokens)` | Full-text corpus stats |

### ULID-to-Seq Mapping

usearch requires `u64` keys, but MAST uses ULIDs (128-bit). The `ulid_to_seq` and `seq_to_ulid` tables provide bidirectional mapping. Each collection maintains an auto-incrementing `seq` counter.

## Index Layer

### Vector Index (usearch)

Per-collection HNSW indexes using cosine distance. Indexes are:

- Held in memory during runtime for fast search
- Serialized to redb's `vector_index` table on `close()` or `Drop`
- Deserialized from redb on `open()` (via `load_from_buffer`)
- After loading, capacity is reserved as `current_size + 1024` to allow inserts

Configuration:

```rust
IndexOptions {
    dimensions: N,
    metric: MetricKind::Cos,
    quantization: ScalarKind::F32,
    ..Default::default()
}
```

The `IndexManager` manages per-collection vector indexes, creating them on first write and loading existing ones from storage.

### Full-Text Index (BM25)

Built-in BM25 implementation stored in redb. No external full-text library.

**Tokenizer:**
1. Lowercase the text
2. Split on non-alphanumeric characters
3. Remove tokens shorter than 2 characters
4. Remove stop words (common English words)

**Posting lists** are stored per-term in the `ft_posting` table. Each entry contains:

```rust
pub struct PostingEntry {
    pub ulid_bytes: [u8; 16],   // Document ID
    pub term_freq: u32,          // Term frequency in this document
    pub field_length: u32,       // Total tokens in this document
}
```

**BM25 scoring** (parameters: `k1=1.2`, `b=0.75`):

```
IDF = ln((N - df + 0.5) / (df + 0.5) + 1.0)
TF  = (tf * (k1 + 1)) / (tf + k1 * (1 - b + b * dl / avgdl))
score = sum(IDF * TF for each query term)
```

### Metadata Index

Inverted index mapping `(collection, key, value)` to lists of ULID bytes. Supports Eq, In, And, and Or filters with arbitrary nesting.

## Serialization

### Memory Serialization (Versioned Envelope)

Memories use a versioned serialization format for forward compatibility:

| Byte 0 | Format |
|--------|--------|
| `0x01` | V1: bincode-serialized full Memory struct |
| Other | V0: raw bincode of legacy struct (no ttl, compacted_at, parent_ids) |

V0 data is auto-migrated on read: `ttl=None`, `compacted_at=None`, `parent_ids=[]`. No scan-and-rewrite migration is needed.

```rust
pub fn serialize_memory(m: &Memory) -> Result<Vec<u8>, MastError> {
    let payload = bincode::serialize(m)?;
    let mut buf = Vec::with_capacity(1 + payload.len());
    buf.push(0x01);  // VERSION_V1
    buf.extend_from_slice(&payload);
    Ok(buf)
}
```

### Edge Serialization

Edges use raw bincode (no versioned envelope).

### Snapshot Format

JSONL (JSON Lines) with `serde_json`. Each line is either a `{"type":"memory","data":{...}}` or `{"type":"edge","data":{...}}` object, preceded by a header line.

## Write Path

1. Generate ULID
2. Compute embedding (if not provided)
3. Write memory to `memories` table (redb transaction)
4. Allocate seq key, write ULID-to-seq and seq-to-ULID mappings
5. Add vector to usearch index (in-memory)
6. Update metadata index (redb)
7. Update full-text index (redb)
8. On failure at step 5: compensating rollback (delete from storage)

## Read Path (Recall)

1. Embed query text (if not pre-computed)
2. Search vector index (or full-text index, or both for hybrid)
3. For vector: apply metadata filter via `filtered_search`
4. Map seq keys back to ULIDs
5. Load memories from storage
6. Filter out compacted and expired memories
7. Compute score: `similarity * tier_weight * time_decay`
8. Sort by score descending, truncate to limit
9. Update access stats on returned memories

## Drop and Flush

`Mast` implements `Drop` to flush all in-memory vector indexes to redb. The `close()` method does the same with error propagation. If neither runs (process crash), vector index updates since the last flush are lost, but all storage data (memories, edges, metadata, full-text) is safe due to redb's ACID guarantees.
