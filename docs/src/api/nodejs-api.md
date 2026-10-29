# Node.js API Reference

Node.js class reference for the `mast-memory` package (napi-rs bindings).

## MastDb

```javascript
const { MastDb } = require('mast-memory');
```

### Constructor

```javascript
new MastDb(path: string)
```

Opens or creates a MAST database at the given path.

---

### store

```typescript
store(
  collection: string,
  content: string,
  embedFn: (text: string) => number[],
  dimensions: number,
  metadata?: Record<string, string>,
  tier?: string,
  ttl?: number,
  embedding?: number[],
  createdAt?: number,
): JsMemory
```

Stores a memory. The `embedFn` receives text and must return a number array.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `collection` | `string` | required | Collection name |
| `content` | `string` | required | Memory text |
| `embedFn` | `function` | required | Embedding function |
| `dimensions` | `number` | required | Vector dimensionality |
| `metadata` | `object` | `undefined` | Key-value metadata |
| `tier` | `string` | `"active"` | "core", "active", "background", "archive" |
| `ttl` | `number` | `undefined` | Time-to-live in seconds |
| `embedding` | `number[]` | `undefined` | Pre-computed vector; skips `embedFn` |
| `createdAt` | `number` | now | When the memory happened, in unix seconds |

---

### recall

```typescript
recall(
  collection: string,
  embedFn: (text: string) => number[],
  dimensions: number,
  query?: string,
  textQuery?: string,
  searchMode?: string,
  limit?: number,
  queryEmbedding?: number[],
  referenceTime?: number,
): JsRecallResult[]
```

Searches for memories. At least one of `query` or `textQuery` is required.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `collection` | `string` | required | Collection to search |
| `embedFn` | `function` | required | Embedding function |
| `dimensions` | `number` | required | Vector dimensionality |
| `query` | `string` | `undefined` | Text query (for vector search) |
| `textQuery` | `string` | `undefined` | Keywords (for full-text search) |
| `searchMode` | `string` | `"vector"` | "vector", "fulltext", "hybrid" |
| `limit` | `number` | `10` | Max results |
| `queryEmbedding` | `number[]` | `undefined` | Pre-computed query vector |
| `referenceTime` | `number` | now | When the question is asked, in unix seconds. Phrases like "two weeks ago" in `query` count back from it |

---

### get

```typescript
get(collection: string, id: string): JsMemory | null
```

Retrieves a memory by ULID string.

---

### delete

```typescript
delete(collection: string, id: string): void
```

Deletes a memory by ULID string.

---

### list

```typescript
list(collection: string): JsMemory[]
```

Returns all memories in a collection.

---

### info

```typescript
info(): JsCollectionInfo[]
```

Returns metadata for all collections.

---

### vacuum

```typescript
vacuum(collection: string): number
```

Removes expired memories. Returns the count of expired memories removed.

---

### relate

```typescript
relate(
  collection: string,
  source: string,
  target: string,
  relation: string,
  weight?: number,
  validFrom?: number,
  validTo?: number,
): string
```

Creates or updates a graph edge. Returns the edge ID as a ULID string. Default weight is 1.0. `validFrom` and `validTo` (unix seconds, `validTo` exclusive) mark when the fact is true; an edge with the same source, target, relation and `validFrom` is updated in place.

---

### unrelate

```typescript
unrelate(collection: string, source: string, target: string, relation: string): void
```

Removes every edge for this source, target and relation, across all validity intervals. Throws if none exists.

---

### invalidate

```typescript
invalidate(
  collection: string,
  source: string,
  target: string,
  relation: string,
  endedAt?: number,
): number
```

Sets `validTo` (default: now) on the open edges for this fact. Returns how many it closed. Throws if no edge is open.

---

### traverse

```typescript
traverse(
  collection: string,
  start: string,
  maxDepth?: number,
  relation?: string,
  minWeight?: number,
  asOf?: number,
): JsEdge[]
```

BFS traversal from a starting entity. Default `maxDepth` is 3. With `asOf` (unix seconds), only edges valid at that time are followed.

---

### snapshot

```typescript
snapshot(collection: string, output: string): JsSnapshotResult
```

Exports a collection to a JSONL file.

---

### restore

```typescript
restore(input: string): JsRestoreResult
```

Imports a collection from a JSONL file. The collection must not already exist.

---

### close

```typescript
close(): void
```

Flushes indexes and closes the database.

---

## Types

### JsMemory

```typescript
interface JsMemory {
  id: string;                    // ULID string
  collection: string;           // Collection name
  content: string;              // Memory text
  metadata: Record<string, string>; // Key-value pairs
  tier: string;                 // Tier name
  createdAt: number;            // Unix timestamp
  accessCount: number;          // Recall count
  lastAccessed: number;         // Last recall timestamp
}
```

### JsRecallResult

```typescript
interface JsRecallResult {
  memory: JsMemory;
  score: number;                // Relevance score (f64)
}
```

### JsCollectionInfo

```typescript
interface JsCollectionInfo {
  name: string;
  dimensions: number;
  memoryCount: number;
}
```

### JsEdge

```typescript
interface JsEdge {
  id: string;
  source: string;
  target: string;
  relation: string;
  weight: number;               // f64
  validFrom?: number;           // Unix seconds; absent = always
  validTo?: number;             // Unix seconds, exclusive; absent = still true
}
```

### JsSnapshotResult

```typescript
interface JsSnapshotResult {
  collection: string;
  memoryCount: number;
  edgeCount: number;
}
```

### JsRestoreResult

```typescript
interface JsRestoreResult {
  collection: string;
  memoryCount: number;
  edgeCount: number;
  warnings: string[];
}
```

## Embedding Function

The embedding function is passed as a `ThreadsafeFunction`. It must accept a `string` and return `number[]`. It is called synchronously from Rust's perspective (via `call_async`).

```javascript
// Simple example
async function embed(text) {
  const response = await openai.embeddings.create({
    input: text,
    model: 'text-embedding-3-small',
  });
  return response.data[0].embedding;
}
```

Note: batch embedding is handled by calling `embed` sequentially in the Node.js bindings (there is no separate `embedBatch` callback).
