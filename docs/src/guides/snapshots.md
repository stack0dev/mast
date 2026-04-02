# Snapshots

MAST supports exporting and importing collections as JSONL (JSON Lines) files. This enables backup, migration, and portability between MAST instances.

## JSONL Format

A snapshot file consists of:

1. **Header line** -- JSON object with metadata
2. **Data lines** -- one JSON object per memory or edge

### Header

```json
{"mast_snapshot":"1.0","collection":"user:alice","memory_count":42,"edge_count":5}
```

| Field | Description |
|-------|-------------|
| `mast_snapshot` | Format version (currently "1.0") |
| `collection` | Collection name |
| `memory_count` | Number of memories |
| `edge_count` | Number of edges |

### Memory Lines

```json
{"type":"memory","data":{"id":"01H...","collection":"user:alice","content":"Alice prefers dark mode","embedding":[0.1,0.2,...],"metadata":{"source":"conversation"},"tier":"Active","created_at":1700000000,"access_count":3,"last_accessed":1700001000,"ttl":null,"compacted_at":null,"parent_ids":[]}}
```

### Edge Lines

```json
{"type":"edge","data":{"id":"01H...","collection":"user:alice","source":"alice","target":"dark_mode","relation":"prefers","weight":0.9,"memory_ids":[],"created_at":1700000000,"updated_at":1700000000}}
```

## Creating a Snapshot

### Rust

```rust
let mut file = std::fs::File::create("backup.jsonl")?;
let result = mast.snapshot("user:alice", &mut file)?;
println!("Exported {} memories, {} edges", result.memory_count, result.edge_count);
```

The `snapshot()` method accepts any `&mut dyn Write`, so you can write to files, buffers, or network streams.

### CLI

```bash
mast snapshot --db agent.mast --collection user:alice --output backup.jsonl
```

### Python

```python
result = db.snapshot("user:alice", "backup.jsonl")
```

### Node.js

```javascript
const result = db.snapshot('user:alice', 'backup.jsonl');
```

## Restoring a Snapshot

### Requirements

- The target collection **must not already exist**. Restore fails if the collection name is already in use.
- The snapshot format version must be "1.0".

### Rust

```rust
let file = std::fs::File::open("backup.jsonl")?;
let mut reader = std::io::BufReader::new(file);
let result = mast.restore(&mut reader)?;

println!("Restored '{}': {} memories, {} edges",
    result.collection, result.memory_count, result.edge_count);

if !result.warnings.is_empty() {
    for w in &result.warnings {
        eprintln!("  warning: {w}");
    }
}
```

### CLI

```bash
mast restore --db agent.mast --input backup.jsonl
```

### Python

```python
result = db.restore("backup.jsonl")
```

### Node.js

```javascript
const result = db.restore('backup.jsonl');
```

## What Gets Exported

- All memories in the collection (including compacted ones)
- All graph edges in the collection
- Embeddings are included in the export

## What Gets Restored

- Memories are written to storage and indexed (vector, full-text, metadata)
- Edges are written to storage with forward/reverse indexes
- Vector indexes are flushed after restore
- Invalid lines produce warnings but do not abort the restore

## Result Types

### SnapshotResult

```rust
pub struct SnapshotResult {
    pub collection: String,
    pub memory_count: u64,
    pub edge_count: u64,
}
```

### RestoreResult

```rust
pub struct RestoreResult {
    pub collection: String,
    pub memory_count: u64,
    pub edge_count: u64,
    pub warnings: Vec<String>,
}
```

## Use Cases

- **Backup:** periodic snapshot to durable storage
- **Migration:** move a collection between MAST instances
- **Testing:** capture a known state for reproducible tests
- **Inspection:** use standard JSON tools to examine memory contents
- **Sharing:** distribute pre-built knowledge bases as JSONL files
