# Search

MAST supports three search modes: **Vector**, **FullText**, and **Hybrid**. All searches are scoped to a single collection and return scored results sorted by relevance.

## Search Modes

```rust
pub enum SearchMode {
    Vector,                              // Cosine similarity via usearch
    FullText,                            // BM25 keyword search
    Hybrid { vector_weight: f32, text_weight: f32 }, // Blended
}
```

## Vector Search

Uses cosine similarity via the usearch HNSW index. Requires either a `query` (text to embed) or a `query_embedding` (pre-computed vector).

```rust
let results = mast.recall(
    RecallRequest {
        collection: "notes".into(),
        query: Some("machine learning concepts".into()),
        query_embedding: None,
        text_query: None,
        search_mode: SearchMode::Vector,
        limit: 10,
        filter: None,
        tier_weights: None,
        time_decay: None,
        reference_time: None,
        time_anchor: None,
    },
    &embedder,
).await?;
```

**Score formula:** `score = similarity * tier_weight * time_decay`

Where:
- `similarity = 1.0 - cosine_distance`
- `tier_weight` from `TierWeights` (Core=1.0, Active=0.9, Background=0.7, Archive=0.5)
- `time_decay = 0.5^(age_secs / half_life_secs)` (1.0 if no decay configured)

## Full-Text Search

Uses BM25 scoring with a built-in tokenizer. The tokenizer lowercases text, splits on non-alphanumeric characters, removes tokens shorter than 2 characters, and filters stop words.

```rust
let results = mast.recall(
    RecallRequest {
        collection: "notes".into(),
        query: None,
        query_embedding: None,
        text_query: Some("dark mode preferences".into()),
        search_mode: SearchMode::FullText,
        limit: 10,
        filter: None,
        tier_weights: None,
        time_decay: None,
        reference_time: None,
        time_anchor: None,
    },
    &embedder,
).await?;
```

**Score formula:** `score = bm25_score * tier_weight * time_decay`

BM25 parameters: `k1 = 1.2`, `b = 0.75`.

Full-text search does not require an embedder. The `text_query` field is used; if absent, `query` is used as fallback.

## Hybrid Search

Runs both vector and full-text search and adds the scores with configurable weights.

```rust
let results = mast.recall(
    RecallRequest {
        collection: "notes".into(),
        query: Some("dark mode".into()),
        query_embedding: None,
        text_query: Some("dark mode".into()),
        search_mode: SearchMode::Hybrid {
            vector_weight: 0.7,
            text_weight: 0.3,
        },
        limit: 10,
        filter: None,
        tier_weights: None,
        time_decay: None,
        reference_time: None,
        time_anchor: None,
    },
    &embedder,
).await?;
```

**Score formula:** `score = vector_weight * vec_score + text_weight * (ft_score / best_ft_score)`

Vector similarity is already on a 0..1 scale and is used as is. BM25 has no fixed scale, so it is divided by the best BM25 score among the candidates. On the LOCOMO benchmark this beat min-max normalization of both lists and reciprocal rank fusion (see `packages/mast-bench/README.md`).

## Time in Recall

Each memory has a `created_at` time. Set `StoreRequest::created_at` when you import history, so the memory carries the time it happened instead of the time it was stored.

`RecallRequest::reference_time` is when the question is asked (default: now). Time decay measures age from it.

When the query contains a relative time phrase, recall boosts memories created near that time:

| Phrase | Target | Window |
|--------|--------|--------|
| "yesterday" | 1 day before | 1 day |
| "today" | reference time | 1 day |
| "recently", "lately" | reference time | 14 days |
| "N days ago" | N days before | 2 days |
| "N weeks ago", "last week" | N weeks before | 7 days |
| "N months ago", "last month" | N × 30 days before | 15 days |
| "N years ago", "last year" | N × 365 days before | 120 days |

`N` can be a digit, a number word up to twelve, "a", "an", "a couple of" or "a few". The boost is `1 + 0.4 * (1 - distance / window)` for memories inside the window and 1.0 outside. To choose the target yourself, set `RecallRequest::time_anchor`.

```rust
let results = mast.recall(
    RecallRequest {
        collection: "chat".into(),
        query: Some("what did we decide two weeks ago".into()),
        reference_time: Some(question_time),
        ..request
    },
    &embedder,
).await?;
```

## Metadata Filtering

All search modes support metadata filtering. Filters are applied after the initial search and before scoring.

```rust
pub enum MetadataFilter {
    Eq(String, String),           // key == value
    In(String, Vec<String>),      // key in [values]
    And(Vec<MetadataFilter>),     // all must match
    Or(Vec<MetadataFilter>),      // any must match
}
```

```rust
let results = mast.recall(
    RecallRequest {
        collection: "notes".into(),
        query: Some("preferences".into()),
        filter: Some(MetadataFilter::And(vec![
            MetadataFilter::Eq("source".into(), "conversation".into()),
            MetadataFilter::In("topic".into(), vec!["ui".into(), "ux".into()]),
        ])),
        ..Default::default()
    },
    &embedder,
).await?;
```

## Tier Weights

Customize how tier membership affects scoring:

```rust
pub struct TierWeights {
    pub core: f32,        // default: 1.0
    pub active: f32,      // default: 0.9
    pub background: f32,  // default: 0.7
    pub archive: f32,     // default: 0.5
}
```

## Time Decay

Apply exponential time decay to favor recent memories:

```rust
pub struct TimeDecay {
    pub half_life_secs: i64,  // time for score to halve
}
```

Example: with `half_life_secs = 86400` (1 day), a 2-day-old memory scores 0.25x vs. a new one.

## Exclusion Rules

Recall automatically excludes:
- Memories with `compacted_at.is_some()` (already compacted)
- Memories with expired TTL (`now - created_at >= ttl`)

## Access Stats

After recall, all returned memories have their `access_count` incremented and `last_accessed` updated. This data feeds into compaction strategies.
