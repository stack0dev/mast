# MAST Memory Plugin for OpenClaw

Long-term memory for OpenClaw powered by [MAST](https://github.com/ericc59/mast) — vector + full-text hybrid search with knowledge graphs.

## Setup

1. Build mast-node native bindings:
   ```bash
   cd crates/mast-node && napi build --release
   ```

2. Install the plugin:
   ```bash
   cd plugins/openclaw && npm install && npm run build
   ```

3. Configure in OpenClaw with your embedding API key:
   ```json
   {
     "embeddingProvider": "voyage",
     "apiKey": "your-voyage-api-key"
   }
   ```
   Or set the `MAST_EMBEDDING_API_KEY` environment variable.

## Tools

### mast_store
Store a memory with optional metadata, tier, and TTL.
```
mast_store({ content: "User prefers dark mode", metadata: { source: "preferences" }, tier: "core" })
```

### mast_search
Search memories using vector, full-text, or hybrid mode.
```
mast_search({ query: "user preferences", limit: 5, mode: "hybrid" })
```

### mast_forget
Delete a memory by ID.
```
mast_forget({ id: "01HXYZ..." })
```

### mast_relate
Create a weighted edge between two nodes in the knowledge graph.
```
mast_relate({ source: "user_prefs", target: "dark_mode", relation: "includes" })
```

### mast_traverse
Walk the knowledge graph from a starting node.
```
mast_traverse({ start: "user_prefs", maxDepth: 2 })
```

### mast_graph
List all edges in the collection's knowledge graph.
```
mast_graph()
```

## Auto-recall & Auto-capture

When enabled (default), the plugin automatically:
- **Recalls** relevant memories before each agent turn and injects them as context
- **Captures** user messages after each turn for future recall

Disable via config:
```json
{ "autoRecall": false, "autoCapture": false }
```

## Config Reference

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `dbPath` | string | `~/.mast/memory.db` | Database file path |
| `collection` | string | `"openclaw"` | Default collection name |
| `embeddingProvider` | `"voyage"` \| `"openai"` | `"voyage"` | Embedding API provider |
| `apiKey` | string | env `MAST_EMBEDDING_API_KEY` | API key for embeddings |
| `embeddingModel` | string | auto | Model name (voyage-3.5 / text-embedding-3-small) |
| `dimensions` | number | 1024 | Embedding dimensions |
| `autoRecall` | boolean | true | Auto-inject memories as context |
| `autoCapture` | boolean | true | Auto-store conversation content |
| `maxResults` | number | 10 | Max results for auto-recall (1-50) |
| `searchMode` | `"vector"` \| `"fulltext"` \| `"hybrid"` | `"hybrid"` | Default search mode |
