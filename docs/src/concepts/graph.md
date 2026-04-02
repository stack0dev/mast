# Graph

MAST includes a graph layer for modeling entity relationships alongside memories. Entities are connected by weighted, typed edges within a collection.

## Data Model

```rust
pub struct Edge {
    pub id: Ulid,                // Unique edge ID
    pub collection: String,      // Scoped to a collection
    pub source: String,          // Source entity name
    pub target: String,          // Target entity name
    pub relation: String,        // Relation type (e.g., "prefers", "knows")
    pub weight: f32,             // Confidence / strength (0.0 to 1.0)
    pub memory_ids: Vec<Ulid>,   // Supporting memories
    pub created_at: i64,         // Creation timestamp
    pub updated_at: i64,         // Last update timestamp
}
```

## Creating Edges

Use `relate()` to create or update an edge. If an edge with the same (source, target, relation) already exists, the weight and memory_ids are updated (upsert).

```rust
let edge_id = mast.relate(RelateRequest {
    collection: "user:alice".into(),
    source: "alice".into(),
    target: "dark_mode".into(),
    relation: "prefers".into(),
    weight: 0.9,
    memory_ids: vec![memory_id],
})?;
```

### Upsert Behavior

```rust
// First call creates the edge
mast.relate(RelateRequest {
    source: "alice".into(),
    target: "dark_mode".into(),
    relation: "prefers".into(),
    weight: 0.7,
    ..
})?;

// Second call updates weight (same source+target+relation)
mast.relate(RelateRequest {
    source: "alice".into(),
    target: "dark_mode".into(),
    relation: "prefers".into(),
    weight: 0.95,  // updated
    ..
})?;
```

## Removing Edges

```rust
mast.unrelate("user:alice", "alice", "dark_mode", "prefers")?;
```

Returns an error if no matching edge exists.

## Traversal

BFS (breadth-first search) traversal from a starting entity:

```rust
let edges = mast.traverse(TraverseRequest {
    collection: "user:alice".into(),
    start: "alice".into(),
    max_depth: 3,
    relation_filter: Some("prefers".into()),  // optional
    min_weight: Some(0.5),                     // optional
})?;

for edge in &edges {
    println!("{} --[{}]--> {} (w={:.2})", edge.source, edge.relation, edge.target, edge.weight);
}
```

### TraverseRequest Fields

| Field | Type | Description |
|-------|------|-------------|
| `collection` | `String` | Collection to search |
| `start` | `String` | Starting entity name |
| `max_depth` | `usize` | Maximum BFS depth |
| `relation_filter` | `Option<String>` | Only follow edges with this relation |
| `min_weight` | `Option<f32>` | Only follow edges above this weight |

Traversal visits each entity at most once (cycle-safe). It returns all edges encountered during BFS, not just the final frontier.

## Subgraph

Get all edges in a collection:

```rust
let graph = mast.subgraph("user:alice")?;
println!("{} edges in collection", graph.edges.len());
```

Returns a `Graph` struct:

```rust
pub struct Graph {
    pub collection: String,
    pub edges: Vec<Edge>,
}
```

## Storage

Edges are stored in redb with three tables:

| Table | Key | Value |
|-------|-----|-------|
| `EDGES` | `(collection, edge_id)` | Serialized `Edge` |
| `EDGE_INDEX_FWD` | `(collection, source, relation)` | List of edge IDs |
| `EDGE_INDEX_REV` | `(collection, target, relation)` | List of edge IDs |

Forward and reverse indexes enable O(1) entity lookups in both directions.

## Linking Edges to Memories

The `memory_ids` field connects an edge to the memories that support it. This creates a bridge between the flat memory store and the entity graph:

```rust
// Extract a relationship from a memory
let memory = mast.store(StoreRequest {
    content: "Alice told me she prefers dark mode".into(),
    ..
}, &embedder).await?;

// Create a graph edge linking back to the memory
mast.relate(RelateRequest {
    source: "alice".into(),
    target: "dark_mode".into(),
    relation: "prefers".into(),
    weight: 0.9,
    memory_ids: vec![memory.id],
    ..
})?;
```
