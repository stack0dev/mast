# Node.js Guide

MAST provides native Node.js bindings via napi-rs, published as the `mast-memory` package.

## Installation

```bash
# From source (requires Rust toolchain)
cd crates/mast-node
npm install
npm run build
```

## Basic Usage

```javascript
const { MastDb } = require('mast-memory');

const db = new MastDb('my_agent.mast');
```

## Embedder Function

The Node.js bindings accept an embedding function and a dimensions count. The function receives a string and must return `number[]`:

```javascript
const OpenAI = require('openai');
const openai = new OpenAI();

async function embed(text) {
  const response = await openai.embeddings.create({
    input: text,
    model: 'text-embedding-3-small',
  });
  return response.data[0].embedding;
}

const DIMS = 1536;
```

## Storing Memories

```javascript
const memory = db.store(
  'user:alice',                    // collection
  'Alice prefers dark mode',       // content
  embed,                           // embed function
  DIMS,                            // dimensions
  { source: 'conversation' },      // metadata (optional)
  'active',                        // tier (optional)
  null,                            // ttl in seconds (optional)
);

console.log(`Stored: ${memory.id}`);
console.log(`Content: ${memory.content}`);
```

## Recalling Memories

```javascript
// Vector search
const results = db.recall(
  'user:alice',     // collection
  embed,            // embed function
  DIMS,             // dimensions
  'UI preferences', // query (optional)
  null,             // text_query (optional)
  'vector',         // search_mode (optional): "vector", "fulltext", "hybrid"
  5,                // limit (optional)
);

for (const r of results) {
  console.log(`[${r.score.toFixed(4)}] ${r.memory.content}`);
}

// Full-text search
const ftResults = db.recall(
  'user:alice',
  embed,
  DIMS,
  null,             // query
  'dark mode',      // text_query
  'fulltext',
  5,
);

// Hybrid search
const hybridResults = db.recall(
  'user:alice',
  embed,
  DIMS,
  'dark mode',      // query (for vector)
  'dark mode',      // text_query (for fulltext)
  'hybrid',
  5,
);
```

## CRUD Operations

```javascript
// Get by ID
const memory = db.get('user:alice', '01H0000000000000000000000');

// Delete by ID
db.delete('user:alice', '01H0000000000000000000000');

// List all memories
const memories = db.list('user:alice');
for (const m of memories) {
  console.log(`${m.id}: ${m.content.substring(0, 60)}`);
}

// Collection info
const collections = db.info();
for (const c of collections) {
  console.log(`${c.name}: ${c.memoryCount} memories, ${c.dimensions} dims`);
}
```

## Graph Operations

```javascript
// Create an edge
const edgeId = db.relate(
  'user:alice',   // collection
  'alice',        // source
  'dark_mode',    // target
  'prefers',      // relation
  0.9,            // weight (optional, default: 1.0)
);

// Remove an edge
db.unrelate('user:alice', 'alice', 'dark_mode', 'prefers');

// Traverse from an entity
const edges = db.traverse(
  'user:alice',   // collection
  'alice',        // start entity
  3,              // max_depth (optional, default: 3)
  'prefers',      // relation filter (optional)
  0.5,            // min_weight (optional)
);

for (const e of edges) {
  console.log(`${e.source} --[${e.relation}]--> ${e.target} (w=${e.weight.toFixed(2)})`);
}
```

## TTL and Vacuum

```javascript
// Store with TTL
db.store('temp', 'ephemeral data', embed, DIMS, null, 'active', 3600);

// Remove expired memories
const expiredCount = db.vacuum('temp');
console.log(`Removed ${expiredCount} expired memories`);
```

## Snapshots

```javascript
// Export a collection
const snapResult = db.snapshot('user:alice', 'backup.jsonl');
console.log(`Exported ${snapResult.memoryCount} memories, ${snapResult.edgeCount} edges`);

// Import (collection must not exist)
const restoreResult = db.restore('backup.jsonl');
console.log(`Restored ${restoreResult.memoryCount} memories`);
```

## Cleanup

```javascript
db.close();
```

## Return Types

### JsMemory

| Property | Type | Description |
|----------|------|-------------|
| `id` | `string` | ULID string |
| `collection` | `string` | Collection name |
| `content` | `string` | Memory text |
| `metadata` | `Record<string, string>` | Key-value pairs |
| `tier` | `string` | "core", "active", "background", "archive" |
| `createdAt` | `number` | Unix timestamp |
| `accessCount` | `number` | Recall count |
| `lastAccessed` | `number` | Last recall timestamp |

### JsRecallResult

| Property | Type | Description |
|----------|------|-------------|
| `memory` | `JsMemory` | The recalled memory |
| `score` | `number` | Relevance score |

### JsCollectionInfo

| Property | Type | Description |
|----------|------|-------------|
| `name` | `string` | Collection name |
| `dimensions` | `number` | Vector dimensionality |
| `memoryCount` | `number` | Number of memories |
