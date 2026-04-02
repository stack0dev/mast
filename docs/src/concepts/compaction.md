# Compaction

Compaction is MAST's mechanism for evolving memories from raw events into distilled knowledge. It promotes memories from one tier to a higher tier by synthesizing multiple memories into new, higher-order insights.

## How It Works

1. **Select candidates** from the source tier based on a strategy
2. **Group into batches** of `max_batch_size`
3. **Synthesize** each batch via the `Compactor` trait (typically an LLM call)
4. **Store** the synthesized memory in the target tier with `parent_ids` linking to sources
5. **Mark** source memories as compacted (`compacted_at` timestamp set)

Compacted memories are hidden from `recall()` but remain in storage for provenance.

## Tiers

| Tier | Purpose | Default Weight |
|------|---------|---------------|
| `Core` | Foundational knowledge, rarely changes | 1.0 |
| `Active` | Recent, frequently accessed | 0.9 |
| `Background` | Older, less accessed | 0.7 |
| `Archive` | Historical, low relevance | 0.5 |

## Compaction Strategies

```rust
pub enum CompactionStrategy {
    /// Compact memories older than a threshold
    Age { older_than_secs: i64 },

    /// Compact memories with zero access count
    AccessDecay,

    /// Combine age (>24h) and low access (<3)
    Hybrid,

    /// Compact specific memory IDs
    Manual { ids: Vec<Ulid> },
}
```

### Age Strategy

Selects memories from the source tier where `now - created_at > older_than_secs`.

### AccessDecay Strategy

Selects memories with `access_count == 0` -- never recalled since stored.

### Hybrid Strategy

Selects memories that are both old (>24 hours) and rarely accessed (<3 times).

### Manual Strategy

Selects specific memories by ID. Useful for explicit curation.

## The Compactor Trait

```rust
#[async_trait]
pub trait Compactor: Send + Sync {
    async fn synthesize(&self, memories: &[Memory]) -> Result<CompactedMemory, MastError>;
}

pub struct CompactedMemory {
    pub content: String,
    pub metadata: HashMap<String, String>,
}
```

The `Compactor` receives a batch of source memories and produces a single synthesized output. This is not summarization -- it is knowledge creation. The implementation typically calls an LLM.

### Example: LLM-Based Compactor

```rust
struct LlmCompactor { client: OpenAIClient }

#[async_trait]
impl Compactor for LlmCompactor {
    async fn synthesize(&self, memories: &[Memory]) -> Result<CompactedMemory, MastError> {
        let prompt = format!(
            "Synthesize these memories into a single higher-order insight:\n{}",
            memories.iter().map(|m| m.content.as_str()).collect::<Vec<_>>().join("\n- ")
        );
        let content = self.client.complete(&prompt).await?;
        Ok(CompactedMemory { content, metadata: HashMap::new() })
    }
}
```

## CompactRequest

```rust
pub struct CompactRequest {
    pub collection: String,
    pub source_tier: Tier,
    pub target_tier: Tier,
    pub strategy: CompactionStrategy,
    pub max_batch_size: usize,   // must be > 0
    pub dry_run: bool,           // preview without writing
}
```

## CompactResult

```rust
pub struct CompactResult {
    pub collection: String,
    pub source_count: usize,         // candidates found
    pub compacted_count: usize,      // memories compacted
    pub new_memory_ids: Vec<Ulid>,   // IDs of synthesized memories
    pub source_memory_ids: Vec<Ulid>, // IDs of source memories
}
```

## Dry Run

Set `dry_run: true` to preview candidates without writing:

```rust
let result = mast.compact(
    CompactRequest {
        collection: "user:alice".into(),
        source_tier: Tier::Active,
        target_tier: Tier::Background,
        strategy: CompactionStrategy::Age { older_than_secs: 86400 },
        max_batch_size: 10,
        dry_run: true,
    },
    &compactor,
    &embedder,
).await?;

println!("{} candidates found", result.source_count);
```

## Provenance

Every compacted memory records its `parent_ids`. You can trace any high-tier memory back to the raw events that produced it:

```rust
let memory = mast.get("user:alice", &compacted_id)?.unwrap();
for parent_id in &memory.parent_ids {
    let parent = mast.get("user:alice", parent_id)?;
    // trace back to source
}
```

## Safety

- `max_batch_size` must be > 0 (validated, returns error otherwise)
- If vector index insertion fails for the synthesized memory, the storage write is rolled back
- Source memories are only marked as compacted after successful synthesis and storage
