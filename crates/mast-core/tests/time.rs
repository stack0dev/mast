//! Event time on stored memories, temporal recall, hybrid scoring, and edge validity.

use mast_core::config::MastConfig;
use mast_core::embed::MockEmbedder;
use mast_core::types::*;
use mast_core::Mast;
use std::collections::HashMap;

const DAY: i64 = 86_400;
const NOW: i64 = 1_700_000_000;

fn open() -> (tempfile::TempDir, Mast) {
    let dir = tempfile::tempdir().unwrap();
    let config = MastConfig::default().with_db_path(dir.path().join("test.mast"));
    (dir, Mast::open(config).unwrap())
}

fn store_at(content: &str, embedding: Vec<f32>, created_at: Option<i64>) -> StoreRequest {
    StoreRequest {
        collection: "c".into(),
        content: content.into(),
        embedding: Some(embedding),
        metadata: HashMap::new(),
        tier: Tier::Active,
        ttl: None,
        created_at,
    }
}

fn recall(query: &str, mode: SearchMode, limit: usize) -> RecallRequest {
    RecallRequest {
        collection: "c".into(),
        query: Some(query.into()),
        query_embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
        text_query: None,
        search_mode: mode,
        limit,
        filter: None,
        tier_weights: None,
        time_decay: None,
        reference_time: Some(NOW),
        time_anchor: None,
    }
}

fn relate(source: &str, target: &str, from: Option<i64>, to: Option<i64>) -> RelateRequest {
    RelateRequest {
        collection: "g".into(),
        source: source.into(),
        target: target.into(),
        relation: "works_at".into(),
        weight: 1.0,
        memory_ids: vec![],
        valid_from: from,
        valid_to: to,
    }
}

fn traverse(as_of: Option<i64>) -> TraverseRequest {
    TraverseRequest {
        collection: "g".into(),
        start: "alice".into(),
        max_depth: 1,
        relation_filter: None,
        min_weight: None,
        as_of,
    }
}

// ---- created_at ----

#[tokio::test]
async fn store_keeps_given_created_at() {
    let (_dir, mut mast) = open();
    let embedder = MockEmbedder::new(4);

    let past = mast
        .store(
            store_at("past", vec![1.0, 0.0, 0.0, 0.0], Some(NOW - 30 * DAY)),
            &embedder,
        )
        .await
        .unwrap();
    assert_eq!(past.created_at, NOW - 30 * DAY);
    assert_eq!(
        mast.get("c", &past.id).unwrap().unwrap().created_at,
        NOW - 30 * DAY
    );

    let present = mast
        .store(
            store_at("present", vec![1.0, 0.0, 0.0, 0.0], None),
            &embedder,
        )
        .await
        .unwrap();
    assert!(present.created_at > NOW);
}

#[tokio::test]
async fn store_batch_keeps_given_created_at() {
    let (_dir, mut mast) = open();
    let embedder = MockEmbedder::new(4);
    let item = |created_at| BatchStoreItem {
        content: "x".into(),
        embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
        metadata: HashMap::new(),
        tier: Tier::Active,
        ttl: None,
        created_at,
    };

    let stored = mast
        .store_batch(
            BatchStoreRequest {
                collection: "c".into(),
                items: vec![item(Some(NOW - DAY)), item(None)],
            },
            &embedder,
        )
        .await
        .unwrap();
    assert_eq!(stored[0].created_at, NOW - DAY);
    assert!(stored[1].created_at > NOW);
}

#[tokio::test]
async fn time_decay_measures_age_from_reference_time() {
    let (_dir, mut mast) = open();
    let embedder = MockEmbedder::new(4);
    mast.store(
        store_at("m", vec![1.0, 0.0, 0.0, 0.0], Some(NOW - 10 * DAY)),
        &embedder,
    )
    .await
    .unwrap();

    let mut request = recall("m", SearchMode::Vector, 1);
    request.time_decay = Some(TimeDecay {
        half_life_secs: 10 * DAY,
    });
    let results = mast.recall(request, &embedder).await.unwrap();
    // exact match (1.0) * active tier (0.9) * one half-life (0.5)
    assert!(
        (results[0].score - 0.45).abs() < 1e-5,
        "score was {}",
        results[0].score
    );
}

// ---- temporal anchor ----

/// Three memories with the same embedding, created 1, 14 and 60 days before NOW.
async fn three_times(mast: &mut Mast) -> [ulid::Ulid; 3] {
    let embedder = MockEmbedder::new(4);
    let mut ids = Vec::new();
    for (content, age) in [("day", 1), ("fortnight", 14), ("months", 60)] {
        let m = mast
            .store(
                store_at(content, vec![1.0, 0.0, 0.0, 0.0], Some(NOW - age * DAY)),
                &embedder,
            )
            .await
            .unwrap();
        ids.push(m.id);
    }
    [ids[0], ids[1], ids[2]]
}

#[tokio::test]
async fn relative_phrase_in_query_boosts_memories_near_that_time() {
    let (_dir, mut mast) = open();
    let embedder = MockEmbedder::new(4);
    let [_, fortnight, months] = three_times(&mut mast).await;

    let results = mast
        .recall(
            recall(
                "what did we talk about two weeks ago",
                SearchMode::Vector,
                3,
            ),
            &embedder,
        )
        .await
        .unwrap();
    assert_eq!(results[0].memory.id, fortnight);
    assert!(results[0].score > results[1].score);

    let results = mast
        .recall(
            recall("what happened 2 months ago", SearchMode::Vector, 3),
            &embedder,
        )
        .await
        .unwrap();
    assert_eq!(results[0].memory.id, months);
}

#[tokio::test]
async fn explicit_anchor_overrides_query_phrase() {
    let (_dir, mut mast) = open();
    let embedder = MockEmbedder::new(4);
    let [day, _, _] = three_times(&mut mast).await;

    let mut request = recall("two weeks ago", SearchMode::Vector, 3);
    request.time_anchor = Some(TimeAnchor {
        target: NOW - DAY,
        window_secs: 2 * DAY,
    });
    let results = mast.recall(request, &embedder).await.unwrap();
    assert_eq!(results[0].memory.id, day);
}

#[tokio::test]
async fn anchor_pulls_a_memory_up_from_beyond_the_limit() {
    let (_dir, mut mast) = open();
    let embedder = MockEmbedder::new(4);
    // 20 closer matches from today, one weaker match from two weeks ago.
    for i in 0..20 {
        mast.store(
            store_at(&format!("today {i}"), vec![1.0, 0.0, 0.0, 0.0], Some(NOW)),
            &embedder,
        )
        .await
        .unwrap();
    }
    let old = mast
        .store(
            store_at("old", vec![0.9, 0.3, 0.0, 0.0], Some(NOW - 14 * DAY)),
            &embedder,
        )
        .await
        .unwrap();

    let without = mast
        .recall(recall("what did we say", SearchMode::Vector, 5), &embedder)
        .await
        .unwrap();
    assert!(without.iter().all(|r| r.memory.id != old.id));

    let with = mast
        .recall(
            recall("what did we say two weeks ago", SearchMode::Vector, 5),
            &embedder,
        )
        .await
        .unwrap();
    assert_eq!(with[0].memory.id, old.id);
}

#[tokio::test]
async fn no_phrase_means_no_boost() {
    let (_dir, mut mast) = open();
    let embedder = MockEmbedder::new(4);
    three_times(&mut mast).await;

    let results = mast
        .recall(
            recall("what did we talk about", SearchMode::Vector, 3),
            &embedder,
        )
        .await
        .unwrap();
    for r in &results {
        assert!((r.score - 0.9).abs() < 1e-5, "score was {}", r.score);
    }
}

// ---- hybrid fusion ----

#[tokio::test]
async fn hybrid_adds_vector_similarity_and_scaled_bm25() {
    let (_dir, mut mast) = open();
    let embedder = MockEmbedder::new(4);
    // "both" matches the embedding and the words; "vector_only" only the embedding.
    let both = mast
        .store(
            store_at("borrow checker lifetimes", vec![1.0, 0.0, 0.0, 0.0], None),
            &embedder,
        )
        .await
        .unwrap();
    let vector_only = mast
        .store(
            store_at("unrelated words", vec![0.6, 0.8, 0.0, 0.0], None),
            &embedder,
        )
        .await
        .unwrap();

    let mode = SearchMode::Hybrid {
        vector_weight: 0.5,
        text_weight: 0.5,
    };
    let results = mast
        .recall(recall("borrow checker", mode, 10), &embedder)
        .await
        .unwrap();

    // Active tier is 0.9. "both": 0.5 * (1.0 * 0.9) + 0.5 * (best BM25 / best BM25).
    assert_eq!(results[0].memory.id, both.id);
    assert!(
        (results[0].score - 0.95).abs() < 1e-5,
        "score was {}",
        results[0].score
    );

    // "vector_only": cosine 0.6, so 0.5 * (0.6 * 0.9). No BM25 part.
    let second = results
        .iter()
        .find(|r| r.memory.id == vector_only.id)
        .unwrap();
    assert!(
        (second.score - 0.27).abs() < 1e-5,
        "score was {}",
        second.score
    );
}

// ---- edge validity ----

#[test]
fn relate_rejects_empty_interval() {
    let (_dir, mast) = open();
    let err = mast
        .relate(relate("alice", "acme", Some(NOW), Some(NOW)))
        .unwrap_err();
    assert!(err.to_string().contains("valid_to"), "{err}");
}

#[test]
fn traverse_as_of_follows_only_valid_edges() {
    let (_dir, mast) = open();
    mast.relate(relate(
        "alice",
        "acme",
        Some(NOW - 100 * DAY),
        Some(NOW - 10 * DAY),
    ))
    .unwrap();
    mast.relate(relate("alice", "globex", Some(NOW - 10 * DAY), None))
        .unwrap();

    let targets = |as_of| -> Vec<String> {
        let mut t: Vec<String> = mast
            .traverse(traverse(as_of))
            .unwrap()
            .into_iter()
            .map(|e| e.target)
            .collect();
        t.sort();
        t
    };

    assert_eq!(targets(Some(NOW - 50 * DAY)), vec!["acme"]);
    // valid_to is exclusive: on the switch day only the new job holds.
    assert_eq!(targets(Some(NOW - 10 * DAY)), vec!["globex"]);
    assert_eq!(targets(Some(NOW)), vec!["globex"]);
    assert!(targets(Some(NOW - 200 * DAY)).is_empty());
    assert_eq!(targets(None), vec!["acme", "globex"]);
}

#[test]
fn invalidate_closes_open_edges() {
    let (_dir, mast) = open();
    mast.relate(relate("alice", "acme", Some(NOW - 100 * DAY), None))
        .unwrap();

    let invalidate = |ended_at| InvalidateRequest {
        collection: "g".into(),
        source: "alice".into(),
        target: "acme".into(),
        relation: "works_at".into(),
        ended_at,
    };

    // Ending before the fact began is rejected.
    assert!(mast.invalidate(invalidate(Some(NOW - 200 * DAY))).is_err());

    assert_eq!(mast.invalidate(invalidate(Some(NOW - DAY))).unwrap(), 1);
    let edges = mast.subgraph("g").unwrap().edges;
    assert_eq!(edges[0].valid_to, Some(NOW - DAY));
    assert!(mast.traverse(traverse(Some(NOW))).unwrap().is_empty());

    // Nothing open is left to close.
    assert!(mast.invalidate(invalidate(None)).is_err());
}

#[test]
fn relate_keeps_one_edge_per_interval() {
    let (_dir, mast) = open();
    let first = mast
        .relate(relate(
            "alice",
            "acme",
            Some(NOW - 100 * DAY),
            Some(NOW - 50 * DAY),
        ))
        .unwrap();
    let second = mast
        .relate(relate("alice", "acme", Some(NOW - 10 * DAY), None))
        .unwrap();
    assert_ne!(first, second);

    // Same interval start again updates in place.
    let mut again = relate("alice", "acme", Some(NOW - 10 * DAY), None);
    again.weight = 0.3;
    assert_eq!(mast.relate(again).unwrap(), second);
    assert_eq!(mast.subgraph("g").unwrap().edges.len(), 2);

    // unrelate removes every interval.
    mast.unrelate("g", "alice", "acme", "works_at").unwrap();
    assert!(mast.subgraph("g").unwrap().edges.is_empty());
    assert!(mast.unrelate("g", "alice", "acme", "works_at").is_err());
}

#[test]
fn snapshot_keeps_edge_validity() {
    let (_dir, mast) = open();
    mast.relate(relate(
        "alice",
        "acme",
        Some(NOW - 100 * DAY),
        Some(NOW - 50 * DAY),
    ))
    .unwrap();
    mast.relate(relate("alice", "acme", Some(NOW - 10 * DAY), None))
        .unwrap();

    let mut buf = Vec::new();
    mast.snapshot("g", &mut buf).unwrap();

    let (_dir2, mut restored) = open();
    let result = restored
        .restore(&mut std::io::Cursor::new(&buf), false)
        .unwrap();
    assert_eq!(result.edge_count, 2);

    let mut edges = restored.subgraph("g").unwrap().edges;
    edges.sort_by_key(|e| e.valid_from);
    assert_eq!(edges[0].valid_from, Some(NOW - 100 * DAY));
    assert_eq!(edges[0].valid_to, Some(NOW - 50 * DAY));
    assert_eq!(edges[1].valid_from, Some(NOW - 10 * DAY));
    assert_eq!(edges[1].valid_to, None);

    // Merging the same snapshot again skips both intervals.
    let again = restored
        .restore(&mut std::io::Cursor::new(&buf), true)
        .unwrap();
    assert_eq!(again.skipped_count, 2);
    assert_eq!(restored.subgraph("g").unwrap().edges.len(), 2);
}

#[test]
fn snapshot_edges_without_validity_still_restore() {
    // A snapshot line written before edges had validity fields.
    let snapshot = concat!(
        r#"{"mast_snapshot":"1.0","collection":"g","memory_count":0,"edge_count":1}"#,
        "\n",
        r#"{"type":"edge","data":{"id":"01HZZZZZZZZZZZZZZZZZZZZZZZ","collection":"g","source":"alice","target":"bob","relation":"knows","weight":1.0,"memory_ids":[],"created_at":1,"updated_at":1}}"#,
        "\n",
    );
    let (_dir, mut mast) = open();
    let result = mast
        .restore(&mut std::io::Cursor::new(snapshot), false)
        .unwrap();
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    let edges = mast.subgraph("g").unwrap().edges;
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].valid_from, None);
    assert_eq!(edges[0].valid_to, None);
}
