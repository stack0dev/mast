# Python API Reference

Python class reference for the `mast-memory` package (PyO3 bindings).

## Mast

```python
from mast import Mast
```

### Constructor

```python
Mast(path: str)
```

Opens or creates a MAST database at the given path.

---

### store

```python
store(
    collection: str,
    content: str,
    embedder: object,
    metadata: dict[str, str] | None = None,
    tier: str | None = None,
    ttl: int | None = None,
    created_at: int | None = None,
) -> PyMemory
```

Stores a memory. The `embedder` must have an `embed(text: str) -> list[float]` method.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `collection` | `str` | required | Collection name |
| `content` | `str` | required | Memory text |
| `embedder` | `object` | required | Embedder with `embed()` method |
| `metadata` | `dict` | `None` | Key-value metadata |
| `tier` | `str` | `"active"` | "core", "active", "background", "archive" |
| `ttl` | `int` | `None` | Time-to-live in seconds |
| `created_at` | `int` | now | When the memory happened, in unix seconds |

---

### recall

```python
recall(
    collection: str,
    embedder: object,
    query: str | None = None,
    text_query: str | None = None,
    search_mode: str | None = None,
    limit: int | None = None,
    filter: dict[str, str] | None = None,
    reference_time: int | None = None,
) -> list[PyRecallResult]
```

Searches for memories. At least one of `query` or `text_query` is required.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `collection` | `str` | required | Collection to search |
| `embedder` | `object` | required | Embedder with `embed()` method |
| `query` | `str` | `None` | Text query (embedded for vector search) |
| `text_query` | `str` | `None` | Keywords for full-text search |
| `search_mode` | `str` | `"vector"` | "vector", "fulltext", "hybrid" |
| `limit` | `int` | `10` | Max results |
| `filter` | `dict` | `None` | Metadata key=value filter (AND semantics) |
| `reference_time` | `int` | now | When the question is asked, in unix seconds. Phrases like "two weeks ago" in `query` count back from it |

---

### get

```python
get(collection: str, id: str) -> PyMemory | None
```

Retrieves a memory by ULID string. Returns `None` if not found.

---

### delete

```python
delete(collection: str, id: str) -> None
```

Deletes a memory by ULID string.

---

### list

```python
list(collection: str) -> list[PyMemory]
```

Returns all memories in a collection.

---

### info

```python
info() -> list[PyCollectionInfo]
```

Returns metadata for all collections.

---

### vacuum

```python
vacuum(collection: str) -> int
```

Removes expired memories. Returns the count of expired memories removed.

---

### relate

```python
relate(
    collection: str,
    source: str,
    target: str,
    relation: str,
    weight: float | None = None,
    valid_from: int | None = None,
    valid_to: int | None = None,
) -> str
```

Creates or updates a graph edge. Returns the edge ID as a ULID string. Default weight is 1.0. `valid_from` and `valid_to` (unix seconds, `valid_to` exclusive) mark when the fact is true; an edge with the same source, target, relation and `valid_from` is updated in place.

---

### unrelate

```python
unrelate(collection: str, source: str, target: str, relation: str) -> None
```

Removes every edge for this source, target and relation, across all validity intervals. Raises `RuntimeError` if none exists.

---

### invalidate

```python
invalidate(
    collection: str,
    source: str,
    target: str,
    relation: str,
    ended_at: int | None = None,
) -> int
```

Sets `valid_to` (default: now) on the open edges for this fact. Returns how many it closed. Raises `RuntimeError` if no edge is open.

---

### traverse

```python
traverse(
    collection: str,
    start: str,
    max_depth: int | None = None,
    relation: str | None = None,
    min_weight: float | None = None,
    as_of: int | None = None,
) -> list[PyEdge]
```

BFS traversal from a starting entity. Default `max_depth` is 3. With `as_of` (unix seconds), only edges valid at that time are followed.

---

### snapshot

```python
snapshot(collection: str, output: str) -> PySnapshotResult
```

Exports a collection to a JSONL file.

---

### restore

```python
restore(input: str) -> PyRestoreResult
```

Imports a collection from a JSONL file. The collection must not already exist.

---

### close

```python
close() -> None
```

Flushes indexes and closes the database.

---

## Data Classes

### PyMemory

| Attribute | Type | Description |
|-----------|------|-------------|
| `id` | `str` | ULID string |
| `collection` | `str` | Collection name |
| `content` | `str` | Memory text |
| `metadata` | `dict[str, str]` | Key-value pairs |
| `tier` | `str` | Tier name |
| `created_at` | `int` | Unix timestamp |
| `access_count` | `int` | Recall count |
| `last_accessed` | `int` | Last recall timestamp |

### PyRecallResult

| Attribute | Type | Description |
|-----------|------|-------------|
| `memory` | `PyMemory` | The recalled memory |
| `score` | `float` | Relevance score |

### PyCollectionInfo

| Attribute | Type | Description |
|-----------|------|-------------|
| `name` | `str` | Collection name |
| `dimensions` | `int` | Vector dimensionality |
| `memory_count` | `int` | Number of memories |

### PyEdge

| Attribute | Type | Description |
|-----------|------|-------------|
| `id` | `str` | Edge ULID |
| `source` | `str` | Source entity |
| `target` | `str` | Target entity |
| `relation` | `str` | Relation type |
| `weight` | `float` | Edge weight |
| `valid_from` | `int \| None` | Start of the time the fact is true, in unix seconds |
| `valid_to` | `int \| None` | End of the time the fact is true, exclusive |

### PySnapshotResult

| Attribute | Type | Description |
|-----------|------|-------------|
| `collection` | `str` | Collection name |
| `memory_count` | `int` | Memories exported |
| `edge_count` | `int` | Edges exported |

### PyRestoreResult

| Attribute | Type | Description |
|-----------|------|-------------|
| `collection` | `str` | Collection name |
| `memory_count` | `int` | Memories imported |
| `edge_count` | `int` | Edges imported |
| `warnings` | `list[str]` | Import warnings |

## Embedder Protocol

Any Python object with these methods:

```python
class Embedder:
    def embed(self, text: str) -> list[float]: ...
    def embed_batch(self, texts: list[str]) -> list[list[float]]: ...  # optional
    def dimensions(self) -> int: ...
```

If `embed_batch` is not defined, MAST falls back to calling `embed` sequentially.
