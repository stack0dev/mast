# Python Guide

MAST provides native Python bindings via PyO3 and maturin, published as the `mast-memory` package.

## Installation

```bash
# From source (requires Rust toolchain + maturin)
cd crates/mast-py
pip install maturin
maturin develop --release
```

## Basic Usage

```python
from mast import Mast

# Open a database
db = Mast("my_agent.mast")
```

## Embedder Protocol

MAST's Python bindings accept any object with an `embed(text) -> list[float]` method and a `dimensions() -> int` method:

```python
class MyEmbedder:
    def embed(self, text: str) -> list[float]:
        # Call your embedding API
        return openai.embeddings.create(input=text, model="text-embedding-3-small").data[0].embedding

    def embed_batch(self, texts: list[str]) -> list[list[float]]:
        # Optional: batch embedding for efficiency
        result = openai.embeddings.create(input=texts, model="text-embedding-3-small")
        return [d.embedding for d in result.data]

    def dimensions(self) -> int:
        return 1536

embedder = MyEmbedder()
```

If `embed_batch` is not defined, MAST falls back to calling `embed` sequentially.

## Storing Memories

```python
memory = db.store(
    collection="user:alice",
    content="Alice prefers dark mode and dislikes notifications",
    embedder=embedder,
    metadata={"source": "conversation", "session": "abc123"},
    tier="active",    # optional, default: "active"
    ttl=3600,         # optional, seconds
)

print(f"Stored: {memory.id}")
print(f"Content: {memory.content}")
print(f"Tier: {memory.tier}")
```

## Recalling Memories

```python
# Vector search
results = db.recall(
    collection="user:alice",
    embedder=embedder,
    query="What are Alice's UI preferences?",
    limit=5,
)

for r in results:
    print(f"[{r.score:.4f}] {r.memory.content}")
    print(f"  id={r.memory.id}, tier={r.memory.tier}")

# Full-text search
results = db.recall(
    collection="user:alice",
    embedder=embedder,
    text_query="dark mode",
    search_mode="fulltext",
)

# Hybrid search
results = db.recall(
    collection="user:alice",
    embedder=embedder,
    query="dark mode",
    text_query="dark mode",
    search_mode="hybrid",
)

# With metadata filter
results = db.recall(
    collection="user:alice",
    embedder=embedder,
    query="preferences",
    filter={"source": "conversation"},
)
```

## CRUD Operations

```python
# Get by ID
memory = db.get("user:alice", "01H0000000000000000000000")

# Delete by ID
db.delete("user:alice", "01H0000000000000000000000")

# List all memories
memories = db.list("user:alice")
for m in memories:
    print(f"{m.id}: {m.content[:60]}")

# Collection info
collections = db.info()
for c in collections:
    print(f"{c.name}: {c.memory_count} memories, {c.dimensions} dims")
```

## Graph Operations

```python
# Create an edge
edge_id = db.relate(
    collection="user:alice",
    source="alice",
    target="dark_mode",
    relation="prefers",
    weight=0.9,
)

# Remove an edge
db.unrelate("user:alice", "alice", "dark_mode", "prefers")

# Traverse from an entity
edges = db.traverse(
    collection="user:alice",
    start="alice",
    max_depth=3,
    relation="prefers",   # optional filter
    min_weight=0.5,       # optional threshold
)

for e in edges:
    print(f"{e.source} --[{e.relation}]--> {e.target} (w={e.weight:.2f})")
```

## TTL and Vacuum

```python
# Store with TTL
db.store(
    collection="temp",
    content="ephemeral data",
    embedder=embedder,
    ttl=3600,  # expires in 1 hour
)

# Remove expired memories
expired_count = db.vacuum("temp")
print(f"Removed {expired_count} expired memories")
```

## Snapshots

```python
# Export a collection
result = db.snapshot("user:alice", "backup.jsonl")
print(f"Exported {result.memory_count} memories, {result.edge_count} edges")

# Import into a new database (collection must not exist)
result = db.restore("backup.jsonl")
print(f"Restored {result.memory_count} memories")
if result.warnings:
    for w in result.warnings:
        print(f"  warning: {w}")
```

## Cleanup

```python
db.close()  # explicit flush
```

## Return Types

### PyMemory

| Attribute | Type | Description |
|-----------|------|-------------|
| `id` | `str` | ULID string |
| `collection` | `str` | Collection name |
| `content` | `str` | Memory text |
| `metadata` | `dict[str, str]` | Key-value pairs |
| `tier` | `str` | "core", "active", "background", "archive" |
| `created_at` | `int` | Unix timestamp |
| `access_count` | `int` | Recall count |
| `last_accessed` | `int` | Last recall timestamp |

### PyRecallResult

| Attribute | Type | Description |
|-----------|------|-------------|
| `memory` | `PyMemory` | The recalled memory |
| `score` | `float` | Relevance score |
