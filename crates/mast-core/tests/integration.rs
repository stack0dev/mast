use mast_core::compact::MockCompactor;
use mast_core::config::MastConfig;
use mast_core::embed::MockEmbedder;
use mast_core::types::*;
use mast_core::Mast;
use std::collections::HashMap;

fn temp_config() -> (tempfile::TempDir, MastConfig) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.mast");
    let config = MastConfig::default().with_db_path(db_path);
    (dir, config)
}

fn assert_collection_count(mast: &Mast, collection: &str, expected: u64) {
    let info = mast.info().unwrap();
    let col = info
        .iter()
        .find(|i| i.name == collection)
        .unwrap_or_else(|| panic!("collection '{collection}' not found"));
    assert_eq!(
        col.memory_count, expected,
        "expected memory_count={expected} for '{collection}', got {}",
        col.memory_count
    );
}

#[tokio::test]
async fn store_and_recall_basic() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(32);
    let mut mast = Mast::open(config).unwrap();

    let memory = mast
        .store(
            StoreRequest {
                collection: "test".into(),
                content: "the sky is blue".into(),
                embedding: None,
                metadata: HashMap::new(),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(memory.collection, "test");
    assert_eq!(memory.content, "the sky is blue");
    assert_eq!(memory.embedding.len(), 32);

    let results = mast
        .recall(
            RecallRequest {
                collection: "test".into(),
                query: Some("blue sky".into()),
                query_embedding: None,
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 5,
                filter: None,
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].memory.id, memory.id);
    assert_eq!(results[0].memory.content, "the sky is blue");

    mast.close().unwrap();
}

#[tokio::test]
async fn store_with_precomputed_embedding() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    let embedding = vec![0.5, 0.5, 0.5, 0.5];
    let memory = mast
        .store(
            StoreRequest {
                collection: "test".into(),
                content: "precomputed".into(),
                embedding: Some(embedding.clone()),
                metadata: HashMap::new(),
                tier: Tier::Core,
                ttl: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(memory.embedding, embedding);
    assert_eq!(memory.tier, Tier::Core);
    mast.close().unwrap();
}

#[tokio::test]
async fn recall_with_precomputed_query_vec() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    mast.store(
        StoreRequest {
            collection: "test".into(),
            content: "close to query".into(),
            embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
            metadata: HashMap::new(),
            tier: Tier::Active,
            ttl: None,
        },
        &embedder,
    )
    .await
    .unwrap();

    mast.store(
        StoreRequest {
            collection: "test".into(),
            content: "far from query".into(),
            embedding: Some(vec![0.0, 0.0, 0.0, 1.0]),
            metadata: HashMap::new(),
            tier: Tier::Active,
            ttl: None,
        },
        &embedder,
    )
    .await
    .unwrap();

    let results = mast
        .recall(
            RecallRequest {
                collection: "test".into(),
                query: None,
                query_embedding: Some(vec![0.9, 0.1, 0.0, 0.0]),
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 10,
                filter: None,
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].memory.content, "close to query");
    assert!(results[0].score > results[1].score);
    mast.close().unwrap();
}

#[tokio::test]
async fn recall_with_metadata_filter() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    mast.store(
        StoreRequest {
            collection: "test".into(),
            content: "red thing".into(),
            embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
            metadata: HashMap::from([("color".into(), "red".into())]),
            tier: Tier::Active,
            ttl: None,
        },
        &embedder,
    )
    .await
    .unwrap();

    mast.store(
        StoreRequest {
            collection: "test".into(),
            content: "blue thing".into(),
            embedding: Some(vec![0.9, 0.1, 0.0, 0.0]),
            metadata: HashMap::from([("color".into(), "blue".into())]),
            tier: Tier::Active,
            ttl: None,
        },
        &embedder,
    )
    .await
    .unwrap();

    let results = mast
        .recall(
            RecallRequest {
                collection: "test".into(),
                query: None,
                query_embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 10,
                filter: Some(MetadataFilter::Eq("color".into(), "blue".into())),
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].memory.content, "blue thing");
    mast.close().unwrap();
}

#[tokio::test]
async fn cross_collection_isolation() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    mast.store(
        StoreRequest {
            collection: "col_a".into(),
            content: "in collection A".into(),
            embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
            metadata: HashMap::new(),
            tier: Tier::Active,
            ttl: None,
        },
        &embedder,
    )
    .await
    .unwrap();

    mast.store(
        StoreRequest {
            collection: "col_b".into(),
            content: "in collection B".into(),
            embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
            metadata: HashMap::new(),
            tier: Tier::Active,
            ttl: None,
        },
        &embedder,
    )
    .await
    .unwrap();

    let results_a = mast
        .recall(
            RecallRequest {
                collection: "col_a".into(),
                query: None,
                query_embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 10,
                filter: None,
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(results_a.len(), 1);
    assert_eq!(results_a[0].memory.content, "in collection A");

    let results_b = mast
        .recall(
            RecallRequest {
                collection: "col_b".into(),
                query: None,
                query_embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 10,
                filter: None,
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(results_b.len(), 1);
    assert_eq!(results_b[0].memory.content, "in collection B");
    mast.close().unwrap();
}

#[tokio::test]
async fn get_memory_by_id() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(16);
    let mut mast = Mast::open(config).unwrap();

    let memory = mast
        .store(
            StoreRequest {
                collection: "test".into(),
                content: "findable".into(),
                embedding: None,
                metadata: HashMap::new(),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    let found = mast.get("test", &memory.id).unwrap().unwrap();
    assert_eq!(found.content, "findable");
    assert_eq!(found.id, memory.id);

    let not_found = mast.get("test", &ulid::Ulid::new()).unwrap();
    assert!(not_found.is_none());
    mast.close().unwrap();
}

#[tokio::test]
async fn info_returns_collections() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(8);
    let mut mast = Mast::open(config).unwrap();

    for _ in 0..3 {
        mast.store(
            StoreRequest {
                collection: "alpha".into(),
                content: "something".into(),
                embedding: None,
                metadata: HashMap::new(),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await
        .unwrap();
    }

    mast.store(
        StoreRequest {
            collection: "beta".into(),
            content: "other".into(),
            embedding: None,
            metadata: HashMap::new(),
            tier: Tier::Active,
            ttl: None,
        },
        &embedder,
    )
    .await
    .unwrap();

    let info = mast.info().unwrap();
    assert_eq!(info.len(), 2);

    let alpha = info.iter().find(|i| i.name == "alpha").unwrap();
    assert_eq!(alpha.memory_count, 3);
    assert_eq!(alpha.dimensions, 8);

    let beta = info.iter().find(|i| i.name == "beta").unwrap();
    assert_eq!(beta.memory_count, 1);
    mast.close().unwrap();
}

#[tokio::test]
async fn close_and_reopen_preserves_data() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("persist.mast");
    let embedder = MockEmbedder::new(4);

    let stored_id;
    {
        let config = MastConfig::default().with_db_path(&db_path);
        let mut mast = Mast::open(config).unwrap();

        let memory = mast
            .store(
                StoreRequest {
                    collection: "persist".into(),
                    content: "durable data".into(),
                    embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                    metadata: HashMap::from([("key".into(), "val".into())]),
                    tier: Tier::Core,
                    ttl: None,
                },
                &embedder,
            )
            .await
            .unwrap();
        stored_id = memory.id;
        mast.close().unwrap();
    }

    {
        let config = MastConfig::default().with_db_path(&db_path);
        let mut mast = Mast::open(config).unwrap();

        let found = mast.get("persist", &stored_id).unwrap().unwrap();
        assert_eq!(found.content, "durable data");
        assert_eq!(found.tier, Tier::Core);
        assert_eq!(found.metadata.get("key").unwrap(), "val");

        let results = mast
            .recall(
                RecallRequest {
                    collection: "persist".into(),
                    query: None,
                    query_embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                    text_query: None,
                    search_mode: SearchMode::Vector,
                    limit: 5,
                    filter: None,
                    tier_weights: None,
                    time_decay: None,
                },
                &embedder,
            )
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].memory.id, stored_id);

        let info = mast.info().unwrap();
        assert_eq!(info.len(), 1);
        assert_eq!(info[0].name, "persist");
        assert_eq!(info[0].memory_count, 1);
    }
}

#[tokio::test]
async fn empty_collection_recall() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    let results = mast
        .recall(
            RecallRequest {
                collection: "empty".into(),
                query: None,
                query_embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 10,
                filter: None,
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert!(results.is_empty());
}

#[tokio::test]
async fn recall_requires_query_or_embedding() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    let result = mast
        .recall(
            RecallRequest {
                collection: "test".into(),
                query: None,
                query_embedding: None,
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 10,
                filter: None,
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn store_multiple_and_recall_ranked() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    for i in 0..10 {
        let mut emb = vec![0.0f32; 4];
        emb[i % 4] = 1.0;
        mast.store(
            StoreRequest {
                collection: "ranked".into(),
                content: format!("memory_{i}"),
                embedding: Some(emb),
                metadata: HashMap::new(),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await
        .unwrap();
    }

    let results = mast
        .recall(
            RecallRequest {
                collection: "ranked".into(),
                query: None,
                query_embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 5,
                filter: None,
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert!(!results.is_empty());
    for w in results.windows(2) {
        assert!(w[0].score >= w[1].score);
    }
    mast.close().unwrap();
}

#[tokio::test]
async fn recall_limit_respected() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    for i in 0..20 {
        mast.store(
            StoreRequest {
                collection: "many".into(),
                content: format!("item_{i}"),
                embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                metadata: HashMap::new(),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await
        .unwrap();
    }

    let results = mast
        .recall(
            RecallRequest {
                collection: "many".into(),
                query: None,
                query_embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 3,
                filter: None,
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 3);
    mast.close().unwrap();
}

// ---- Edge case tests ----

#[tokio::test]
async fn store_empty_embedding_rejected() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    let result = mast
        .store(
            StoreRequest {
                collection: "test".into(),
                content: "bad".into(),
                embedding: Some(vec![]),
                metadata: HashMap::new(),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("empty"), "expected empty error, got: {err}");
}

#[tokio::test]
async fn store_dimension_mismatch_rejected() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    mast.store(
        StoreRequest {
            collection: "test".into(),
            content: "first".into(),
            embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
            metadata: HashMap::new(),
            tier: Tier::Active,
            ttl: None,
        },
        &embedder,
    )
    .await
    .unwrap();

    let result = mast
        .store(
            StoreRequest {
                collection: "test".into(),
                content: "second".into(),
                embedding: Some(vec![1.0, 0.0]),
                metadata: HashMap::new(),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("dimensions"),
        "expected dimension error, got: {err}"
    );
}

#[tokio::test]
async fn recall_query_dimension_mismatch_rejected() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    mast.store(
        StoreRequest {
            collection: "test".into(),
            content: "stored".into(),
            embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
            metadata: HashMap::new(),
            tier: Tier::Active,
            ttl: None,
        },
        &embedder,
    )
    .await
    .unwrap();

    let result = mast
        .recall(
            RecallRequest {
                collection: "test".into(),
                query: None,
                query_embedding: Some(vec![1.0, 0.0]),
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 5,
                filter: None,
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("dimensions"),
        "expected dimension error, got: {err}"
    );
}

#[tokio::test]
async fn recall_empty_query_embedding_rejected() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    let result = mast
        .recall(
            RecallRequest {
                collection: "test".into(),
                query: None,
                query_embedding: Some(vec![]),
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 5,
                filter: None,
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("empty"), "expected empty error, got: {err}");
}

#[tokio::test]
async fn recall_limit_zero_uses_default() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    for i in 0..15 {
        mast.store(
            StoreRequest {
                collection: "test".into(),
                content: format!("item_{i}"),
                embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                metadata: HashMap::new(),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await
        .unwrap();
    }

    let results = mast
        .recall(
            RecallRequest {
                collection: "test".into(),
                query: None,
                query_embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 0,
                filter: None,
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 10);
}

#[tokio::test]
async fn store_empty_content() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    let memory = mast
        .store(
            StoreRequest {
                collection: "test".into(),
                content: "".into(),
                embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                metadata: HashMap::new(),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(memory.content, "");
    let found = mast.get("test", &memory.id).unwrap().unwrap();
    assert_eq!(found.content, "");
    mast.close().unwrap();
}

#[tokio::test]
async fn get_from_wrong_collection_returns_none() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    let memory = mast
        .store(
            StoreRequest {
                collection: "col_a".into(),
                content: "in A".into(),
                embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                metadata: HashMap::new(),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    let result = mast.get("col_b", &memory.id).unwrap();
    assert!(result.is_none());
    mast.close().unwrap();
}

#[tokio::test]
async fn metadata_filter_in_through_recall() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    for (content, color) in [("a", "red"), ("b", "blue"), ("c", "green")] {
        mast.store(
            StoreRequest {
                collection: "test".into(),
                content: content.into(),
                embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                metadata: HashMap::from([("color".into(), color.into())]),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await
        .unwrap();
    }

    let results = mast
        .recall(
            RecallRequest {
                collection: "test".into(),
                query: None,
                query_embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 10,
                filter: Some(MetadataFilter::In(
                    "color".into(),
                    vec!["red".into(), "green".into()],
                )),
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 2);
    let contents: Vec<&str> = results.iter().map(|r| r.memory.content.as_str()).collect();
    assert!(contents.contains(&"a"));
    assert!(contents.contains(&"c"));
    mast.close().unwrap();
}

#[tokio::test]
async fn metadata_filter_and_through_recall() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    mast.store(
        StoreRequest {
            collection: "test".into(),
            content: "match".into(),
            embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
            metadata: HashMap::from([
                ("color".into(), "red".into()),
                ("size".into(), "large".into()),
            ]),
            tier: Tier::Active,
            ttl: None,
        },
        &embedder,
    )
    .await
    .unwrap();

    mast.store(
        StoreRequest {
            collection: "test".into(),
            content: "no_match".into(),
            embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
            metadata: HashMap::from([
                ("color".into(), "red".into()),
                ("size".into(), "small".into()),
            ]),
            tier: Tier::Active,
            ttl: None,
        },
        &embedder,
    )
    .await
    .unwrap();

    let results = mast
        .recall(
            RecallRequest {
                collection: "test".into(),
                query: None,
                query_embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 10,
                filter: Some(MetadataFilter::And(vec![
                    MetadataFilter::Eq("color".into(), "red".into()),
                    MetadataFilter::Eq("size".into(), "large".into()),
                ])),
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].memory.content, "match");
    mast.close().unwrap();
}

#[tokio::test]
async fn metadata_filter_matches_nothing() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    mast.store(
        StoreRequest {
            collection: "test".into(),
            content: "exists".into(),
            embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
            metadata: HashMap::from([("key".into(), "val".into())]),
            tier: Tier::Active,
            ttl: None,
        },
        &embedder,
    )
    .await
    .unwrap();

    let results = mast
        .recall(
            RecallRequest {
                collection: "test".into(),
                query: None,
                query_embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 10,
                filter: Some(MetadataFilter::Eq("key".into(), "nonexistent".into())),
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert!(results.is_empty());
    mast.close().unwrap();
}

#[tokio::test]
async fn reopen_preserves_metadata_index() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("meta_persist.mast");
    let embedder = MockEmbedder::new(4);

    {
        let config = MastConfig::default().with_db_path(&db_path);
        let mut mast = Mast::open(config).unwrap();

        mast.store(
            StoreRequest {
                collection: "test".into(),
                content: "tagged".into(),
                embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                metadata: HashMap::from([("env".into(), "prod".into())]),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await
        .unwrap();

        mast.store(
            StoreRequest {
                collection: "test".into(),
                content: "untagged".into(),
                embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                metadata: HashMap::new(),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await
        .unwrap();

        mast.close().unwrap();
    }

    {
        let config = MastConfig::default().with_db_path(&db_path);
        let mut mast = Mast::open(config).unwrap();

        let results = mast
            .recall(
                RecallRequest {
                    collection: "test".into(),
                    query: None,
                    query_embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                    text_query: None,
                    search_mode: SearchMode::Vector,
                    limit: 10,
                    filter: Some(MetadataFilter::Eq("env".into(), "prod".into())),
                    tier_weights: None,
                    time_decay: None,
                },
                &embedder,
            )
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].memory.content, "tagged");
    }
}

#[tokio::test]
async fn store_after_reopen_continues_seq() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("seq_persist.mast");
    let embedder = MockEmbedder::new(4);

    {
        let config = MastConfig::default().with_db_path(&db_path);
        let mut mast = Mast::open(config).unwrap();

        mast.store(
            StoreRequest {
                collection: "test".into(),
                content: "first".into(),
                embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                metadata: HashMap::new(),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await
        .unwrap();

        mast.close().unwrap();
    }

    {
        let config = MastConfig::default().with_db_path(&db_path);
        let mut mast = Mast::open(config).unwrap();

        mast.store(
            StoreRequest {
                collection: "test".into(),
                content: "second".into(),
                embedding: Some(vec![0.0, 1.0, 0.0, 0.0]),
                metadata: HashMap::new(),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await
        .unwrap();

        let results = mast
            .recall(
                RecallRequest {
                    collection: "test".into(),
                    query: None,
                    query_embedding: Some(vec![0.5, 0.5, 0.0, 0.0]),
                    text_query: None,
                    search_mode: SearchMode::Vector,
                    limit: 10,
                    filter: None,
                    tier_weights: None,
                    time_decay: None,
                },
                &embedder,
            )
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        mast.close().unwrap();
    }
}

#[tokio::test]
async fn unicode_content_and_metadata() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    let memory = mast
        .store(
            StoreRequest {
                collection: "user:alice".into(),
                content: "Aliceは暗いモードを好む".into(),
                embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                metadata: HashMap::from([
                    ("lang".into(), "ja".into()),
                    ("note".into(), "日本語テスト".into()),
                ]),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    let found = mast.get("user:alice", &memory.id).unwrap().unwrap();
    assert_eq!(found.content, "Aliceは暗いモードを好む");
    assert_eq!(found.metadata.get("note").unwrap(), "日本語テスト");

    let results = mast
        .recall(
            RecallRequest {
                collection: "user:alice".into(),
                query: None,
                query_embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 10,
                filter: Some(MetadataFilter::Eq("note".into(), "日本語テスト".into())),
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    mast.close().unwrap();
}

#[tokio::test]
async fn many_metadata_pairs() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    let mut metadata = HashMap::new();
    for i in 0..50 {
        metadata.insert(format!("key_{i}"), format!("val_{i}"));
    }

    let memory = mast
        .store(
            StoreRequest {
                collection: "test".into(),
                content: "lots of metadata".into(),
                embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                metadata: metadata.clone(),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    let found = mast.get("test", &memory.id).unwrap().unwrap();
    assert_eq!(found.metadata.len(), 50);
    assert_eq!(found.metadata.get("key_25").unwrap(), "val_25");

    let results = mast
        .recall(
            RecallRequest {
                collection: "test".into(),
                query: None,
                query_embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 10,
                filter: Some(MetadataFilter::Eq("key_42".into(), "val_42".into())),
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    mast.close().unwrap();
}

#[tokio::test]
async fn embedder_failure_propagates() {
    use mast_core::error::MastError;

    struct FailingEmbedder;

    #[async_trait::async_trait]
    impl mast_core::embed::Embedder for FailingEmbedder {
        async fn embed(&self, _text: &str) -> Result<Vec<f32>, MastError> {
            Err(MastError::Embedder("network timeout".into()))
        }
        async fn embed_batch(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, MastError> {
            Err(MastError::Embedder("network timeout".into()))
        }
        fn dimensions(&self) -> usize {
            4
        }
    }

    let (_dir, config) = temp_config();
    let mut mast = Mast::open(config).unwrap();
    let embedder = FailingEmbedder;

    let result = mast
        .store(
            StoreRequest {
                collection: "test".into(),
                content: "needs embedding".into(),
                embedding: None,
                metadata: HashMap::new(),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("network timeout"), "got: {err}");
}

#[tokio::test]
async fn recall_embedder_failure_propagates() {
    use mast_core::error::MastError;

    struct FailingEmbedder;

    #[async_trait::async_trait]
    impl mast_core::embed::Embedder for FailingEmbedder {
        async fn embed(&self, _text: &str) -> Result<Vec<f32>, MastError> {
            Err(MastError::Embedder("api down".into()))
        }
        async fn embed_batch(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, MastError> {
            Err(MastError::Embedder("api down".into()))
        }
        fn dimensions(&self) -> usize {
            4
        }
    }

    let (_dir, config) = temp_config();
    let mut mast = Mast::open(config).unwrap();
    let embedder = FailingEmbedder;

    let result = mast
        .recall(
            RecallRequest {
                collection: "test".into(),
                query: Some("hello".into()),
                query_embedding: None,
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 5,
                filter: None,
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("api down"), "got: {err}");
}

// ---- Drop impl test ----

#[tokio::test]
async fn drop_preserves_index() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("drop.mast");
    let embedder = MockEmbedder::new(4);

    // Store and drop (no explicit close) — Drop should flush
    {
        let config = MastConfig::default().with_db_path(&db_path);
        let mut mast = Mast::open(config).unwrap();

        mast.store(
            StoreRequest {
                collection: "test".into(),
                content: "persisted via drop".into(),
                embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                metadata: HashMap::new(),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await
        .unwrap();
        // No close() — relies on Drop
    }

    // Reopen — vector index should be present because Drop flushed it
    {
        let config = MastConfig::default().with_db_path(&db_path);
        let mut mast = Mast::open(config).unwrap();

        let results = mast
            .recall(
                RecallRequest {
                    collection: "test".into(),
                    query: None,
                    query_embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                    text_query: None,
                    search_mode: SearchMode::Vector,
                    limit: 10,
                    filter: None,
                    tier_weights: None,
                    time_decay: None,
                },
                &embedder,
            )
            .await
            .unwrap();
        assert_eq!(
            results.len(),
            1,
            "expected 1 result after drop (index should be preserved), got {}",
            results.len()
        );
    }
}

#[tokio::test]
async fn query_embedding_takes_precedence_over_query() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    mast.store(
        StoreRequest {
            collection: "test".into(),
            content: "target".into(),
            embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
            metadata: HashMap::new(),
            tier: Tier::Active,
            ttl: None,
        },
        &embedder,
    )
    .await
    .unwrap();

    let results = mast
        .recall(
            RecallRequest {
                collection: "test".into(),
                query: Some("this text would produce a different vector".into()),
                query_embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 10,
                filter: None,
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert!(
        results[0].score > 0.89,
        "expected high score from query_embedding, got {}",
        results[0].score
    );
    mast.close().unwrap();
}

#[tokio::test]
async fn precomputed_store_bypasses_failing_embedder() {
    use mast_core::error::MastError;

    struct PanicEmbedder;

    #[async_trait::async_trait]
    impl mast_core::embed::Embedder for PanicEmbedder {
        async fn embed(&self, _text: &str) -> Result<Vec<f32>, MastError> {
            panic!("embedder should not be called when embedding is precomputed");
        }
        async fn embed_batch(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, MastError> {
            panic!("embedder should not be called when embedding is precomputed");
        }
        fn dimensions(&self) -> usize {
            4
        }
    }

    let (_dir, config) = temp_config();
    let mut mast = Mast::open(config).unwrap();

    let memory = mast
        .store(
            StoreRequest {
                collection: "test".into(),
                content: "precomputed".into(),
                embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                metadata: HashMap::new(),
                tier: Tier::Active,
                ttl: None,
            },
            &PanicEmbedder,
        )
        .await
        .unwrap();

    assert_eq!(memory.content, "precomputed");
    mast.close().unwrap();
}

#[tokio::test]
async fn precomputed_recall_bypasses_failing_embedder() {
    use mast_core::error::MastError;

    struct PanicEmbedder;

    #[async_trait::async_trait]
    impl mast_core::embed::Embedder for PanicEmbedder {
        async fn embed(&self, _text: &str) -> Result<Vec<f32>, MastError> {
            panic!("embedder should not be called when query_embedding is precomputed");
        }
        async fn embed_batch(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, MastError> {
            panic!("embedder should not be called");
        }
        fn dimensions(&self) -> usize {
            4
        }
    }

    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    mast.store(
        StoreRequest {
            collection: "test".into(),
            content: "stored".into(),
            embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
            metadata: HashMap::new(),
            tier: Tier::Active,
            ttl: None,
        },
        &embedder,
    )
    .await
    .unwrap();

    let results = mast
        .recall(
            RecallRequest {
                collection: "test".into(),
                query: None,
                query_embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 10,
                filter: None,
                tier_weights: None,
                time_decay: None,
            },
            &PanicEmbedder,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    mast.close().unwrap();
}

#[tokio::test]
async fn embedder_returns_wrong_dimensions_on_store() {
    use mast_core::error::MastError;

    struct WrongDimsEmbedder;

    #[async_trait::async_trait]
    impl mast_core::embed::Embedder for WrongDimsEmbedder {
        async fn embed(&self, _text: &str) -> Result<Vec<f32>, MastError> {
            Ok(vec![1.0, 0.0])
        }
        async fn embed_batch(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, MastError> {
            Ok(vec![vec![1.0, 0.0]])
        }
        fn dimensions(&self) -> usize {
            2
        }
    }

    let (_dir, config) = temp_config();
    let mock = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    mast.store(
        StoreRequest {
            collection: "test".into(),
            content: "four dims".into(),
            embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
            metadata: HashMap::new(),
            tier: Tier::Active,
            ttl: None,
        },
        &mock,
    )
    .await
    .unwrap();

    let result = mast
        .store(
            StoreRequest {
                collection: "test".into(),
                content: "will fail".into(),
                embedding: None,
                metadata: HashMap::new(),
                tier: Tier::Active,
                ttl: None,
            },
            &WrongDimsEmbedder,
        )
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("dimensions"), "got: {err}");
    mast.close().unwrap();
}

#[tokio::test]
async fn embedder_returns_wrong_dimensions_on_recall() {
    use mast_core::error::MastError;

    struct WrongDimsEmbedder;

    #[async_trait::async_trait]
    impl mast_core::embed::Embedder for WrongDimsEmbedder {
        async fn embed(&self, _text: &str) -> Result<Vec<f32>, MastError> {
            Ok(vec![1.0, 0.0])
        }
        async fn embed_batch(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, MastError> {
            Ok(vec![vec![1.0, 0.0]])
        }
        fn dimensions(&self) -> usize {
            2
        }
    }

    let (_dir, config) = temp_config();
    let mock = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    mast.store(
        StoreRequest {
            collection: "test".into(),
            content: "four dims".into(),
            embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
            metadata: HashMap::new(),
            tier: Tier::Active,
            ttl: None,
        },
        &mock,
    )
    .await
    .unwrap();

    let result = mast
        .recall(
            RecallRequest {
                collection: "test".into(),
                query: Some("search".into()),
                query_embedding: None,
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 5,
                filter: None,
                tier_weights: None,
                time_decay: None,
            },
            &WrongDimsEmbedder,
        )
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("dimensions"), "got: {err}");
    mast.close().unwrap();
}

// ---- Phase 2 tests ----

#[tokio::test]
async fn store_delete_recall() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    let mem = mast
        .store(
            StoreRequest {
                collection: "test".into(),
                content: "deletable".into(),
                embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                metadata: HashMap::from([("k".into(), "v".into())]),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    mast.delete("test", &mem.id).unwrap();

    assert!(mast.get("test", &mem.id).unwrap().is_none());

    let results = mast
        .recall(
            RecallRequest {
                collection: "test".into(),
                query: None,
                query_embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 10,
                filter: None,
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await
        .unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn store_list() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    for i in 0..3 {
        mast.store(
            StoreRequest {
                collection: "test".into(),
                content: format!("item_{i}"),
                embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                metadata: HashMap::new(),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await
        .unwrap();
    }

    let memories = mast.list("test").unwrap();
    assert_eq!(memories.len(), 3);
}

#[tokio::test]
async fn recall_updates_access_stats() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    let mem = mast
        .store(
            StoreRequest {
                collection: "test".into(),
                content: "tracked".into(),
                embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                metadata: HashMap::new(),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(mem.access_count, 0);

    // Recall should increment access_count
    mast.recall(
        RecallRequest {
            collection: "test".into(),
            query: None,
            query_embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
            text_query: None,
            search_mode: SearchMode::Vector,
            limit: 10,
            filter: None,
            tier_weights: None,
            time_decay: None,
        },
        &embedder,
    )
    .await
    .unwrap();

    // Access stats are deferred — flush before checking
    mast.flush_access_stats().unwrap();

    let updated = mast.get("test", &mem.id).unwrap().unwrap();
    assert_eq!(updated.access_count, 1);
}

#[tokio::test]
async fn recall_with_tier_weights() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    // Store a Core and an Archive memory with same embedding
    mast.store(
        StoreRequest {
            collection: "test".into(),
            content: "core mem".into(),
            embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
            metadata: HashMap::new(),
            tier: Tier::Core,
            ttl: None,
        },
        &embedder,
    )
    .await
    .unwrap();

    mast.store(
        StoreRequest {
            collection: "test".into(),
            content: "archive mem".into(),
            embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
            metadata: HashMap::new(),
            tier: Tier::Archive,
            ttl: None,
        },
        &embedder,
    )
    .await
    .unwrap();

    let results = mast
        .recall(
            RecallRequest {
                collection: "test".into(),
                query: None,
                query_embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 10,
                filter: None,
                tier_weights: Some(TierWeights {
                    core: 1.0,
                    active: 0.9,
                    background: 0.7,
                    archive: 0.1,
                }),
                time_decay: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 2);
    // Core should score higher than Archive due to weight
    assert_eq!(results[0].memory.content, "core mem");
    assert!(results[0].score > results[1].score);
}

#[tokio::test]
async fn recall_with_time_decay() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    // Store two memories with same embedding — both will be recent, so decay should be minimal
    mast.store(
        StoreRequest {
            collection: "test".into(),
            content: "recent".into(),
            embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
            metadata: HashMap::new(),
            tier: Tier::Active,
            ttl: None,
        },
        &embedder,
    )
    .await
    .unwrap();

    let results = mast
        .recall(
            RecallRequest {
                collection: "test".into(),
                query: None,
                query_embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 10,
                filter: None,
                tier_weights: None,
                time_decay: Some(TimeDecay {
                    half_life_secs: 3600,
                }),
            },
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    // Score should be close to similarity * 0.9 (tier weight) * ~1.0 (recent, so minimal decay)
    assert!(results[0].score > 0.5);
}

#[tokio::test]
async fn ttl_filtered_in_recall() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    // Store memory with very short TTL (1 second) — created_at is "now"
    // Since created_at is auto-set to now, the TTL won't expire immediately
    // But we can store with a TTL and verify it's set
    let mem = mast
        .store(
            StoreRequest {
                collection: "test".into(),
                content: "ephemeral".into(),
                embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                metadata: HashMap::new(),
                tier: Tier::Active,
                ttl: Some(999999), // long TTL — should not expire
            },
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(mem.ttl, Some(999999));

    let results = mast
        .recall(
            RecallRequest {
                collection: "test".into(),
                query: None,
                query_embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 10,
                filter: None,
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
}

#[tokio::test]
async fn vacuum_removes_expired() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    // Store a memory with TTL of 1 second
    mast.store(
        StoreRequest {
            collection: "test".into(),
            content: "will expire".into(),
            embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
            metadata: HashMap::new(),
            tier: Tier::Active,
            ttl: Some(0), // TTL = 0 means already expired
        },
        &embedder,
    )
    .await
    .unwrap();

    // Store a memory without TTL
    mast.store(
        StoreRequest {
            collection: "test".into(),
            content: "no expiry".into(),
            embedding: Some(vec![0.0, 1.0, 0.0, 0.0]),
            metadata: HashMap::new(),
            tier: Tier::Active,
            ttl: None,
        },
        &embedder,
    )
    .await
    .unwrap();

    // Brief wait to ensure now > created_at
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    let result = mast.vacuum("test").unwrap();
    assert_eq!(result.expired_count, 1);

    let remaining = mast.list("test").unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].content, "no expiry");
}

#[tokio::test]
async fn compact_basic() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let compactor = MockCompactor;
    let mut mast = Mast::open(config).unwrap();

    // Store memories — they'll have created_at = now, so use Manual strategy
    let m1 = mast
        .store(
            StoreRequest {
                collection: "test".into(),
                content: "alpha".into(),
                embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                metadata: HashMap::new(),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    let m2 = mast
        .store(
            StoreRequest {
                collection: "test".into(),
                content: "beta".into(),
                embedding: Some(vec![0.0, 1.0, 0.0, 0.0]),
                metadata: HashMap::new(),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    let result = mast
        .compact(
            CompactRequest {
                collection: "test".into(),
                source_tier: Tier::Active,
                target_tier: Tier::Archive,
                strategy: CompactionStrategy::Manual {
                    ids: vec![m1.id, m2.id],
                },
                max_batch_size: 10,
                dry_run: false,
            },
            &compactor,
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(result.source_count, 2);
    assert_eq!(result.compacted_count, 2);
    assert_eq!(result.new_memory_ids.len(), 1);

    // Source memories should be marked compacted
    let m1_after = mast.get("test", &m1.id).unwrap().unwrap();
    assert!(m1_after.compacted_at.is_some());

    // New compacted memory should exist
    let new_mem = mast.get("test", &result.new_memory_ids[0]).unwrap().unwrap();
    assert_eq!(new_mem.content, "alpha | beta");
    assert_eq!(new_mem.tier, Tier::Archive);
    assert_eq!(new_mem.parent_ids.len(), 2);
}

#[tokio::test]
async fn compact_dry_run() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let compactor = MockCompactor;
    let mut mast = Mast::open(config).unwrap();

    let m1 = mast
        .store(
            StoreRequest {
                collection: "test".into(),
                content: "alpha".into(),
                embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                metadata: HashMap::new(),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    let result = mast
        .compact(
            CompactRequest {
                collection: "test".into(),
                source_tier: Tier::Active,
                target_tier: Tier::Archive,
                strategy: CompactionStrategy::Manual {
                    ids: vec![m1.id],
                },
                max_batch_size: 10,
                dry_run: true,
            },
            &compactor,
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(result.source_count, 1);
    assert_eq!(result.compacted_count, 0);
    assert!(result.new_memory_ids.is_empty());

    // Source should NOT be marked compacted
    let m1_after = mast.get("test", &m1.id).unwrap().unwrap();
    assert!(m1_after.compacted_at.is_none());
}

#[tokio::test]
async fn compact_skips_already_compacted() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let compactor = MockCompactor;
    let mut mast = Mast::open(config).unwrap();

    let m1 = mast
        .store(
            StoreRequest {
                collection: "test".into(),
                content: "alpha".into(),
                embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                metadata: HashMap::new(),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    // Compact once
    mast.compact(
        CompactRequest {
            collection: "test".into(),
            source_tier: Tier::Active,
            target_tier: Tier::Archive,
            strategy: CompactionStrategy::Manual {
                ids: vec![m1.id],
            },
            max_batch_size: 10,
            dry_run: false,
        },
        &compactor,
        &embedder,
    )
    .await
    .unwrap();

    // Compact again — m1 should be skipped
    let result = mast
        .compact(
            CompactRequest {
                collection: "test".into(),
                source_tier: Tier::Active,
                target_tier: Tier::Archive,
                strategy: CompactionStrategy::Manual {
                    ids: vec![m1.id],
                },
                max_batch_size: 10,
                dry_run: false,
            },
            &compactor,
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(result.source_count, 0);
    assert_eq!(result.compacted_count, 0);
}

#[tokio::test]
async fn compacted_excluded_from_recall() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let compactor = MockCompactor;
    let mut mast = Mast::open(config).unwrap();

    let m1 = mast
        .store(
            StoreRequest {
                collection: "test".into(),
                content: "compacted source".into(),
                embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                metadata: HashMap::new(),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    mast.compact(
        CompactRequest {
            collection: "test".into(),
            source_tier: Tier::Active,
            target_tier: Tier::Archive,
            strategy: CompactionStrategy::Manual {
                ids: vec![m1.id],
            },
            max_batch_size: 10,
            dry_run: false,
        },
        &compactor,
        &embedder,
    )
    .await
    .unwrap();

    let results = mast
        .recall(
            RecallRequest {
                collection: "test".into(),
                query: None,
                query_embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 10,
                filter: None,
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    // Only the new compacted memory should appear, not the original
    for r in &results {
        assert!(
            r.memory.compacted_at.is_none(),
            "compacted memory should not appear in recall"
        );
    }

    // But list() still shows them with compacted_at set
    let all = mast.list("test").unwrap();
    let compacted: Vec<_> = all.iter().filter(|m| m.compacted_at.is_some()).collect();
    assert_eq!(compacted.len(), 1);
}

// ---- Graph tests ----

#[tokio::test]
async fn relate_and_traverse() {
    let (_dir, config) = temp_config();
    let mast = Mast::open(config).unwrap();

    mast.relate(RelateRequest {
        collection: "test".into(),
        source: "alice".into(),
        target: "bob".into(),
        relation: "knows".into(),
        weight: 0.9,
        memory_ids: vec![],
    })
    .unwrap();

    mast.relate(RelateRequest {
        collection: "test".into(),
        source: "bob".into(),
        target: "charlie".into(),
        relation: "knows".into(),
        weight: 0.8,
        memory_ids: vec![],
    })
    .unwrap();

    let edges = mast
        .traverse(TraverseRequest {
            collection: "test".into(),
            start: "alice".into(),
            max_depth: 3,
            relation_filter: None,
            min_weight: None,
        })
        .unwrap();

    assert_eq!(edges.len(), 2);
}

#[tokio::test]
async fn relate_upsert() {
    let (_dir, config) = temp_config();
    let mast = Mast::open(config).unwrap();

    let id1 = mast
        .relate(RelateRequest {
            collection: "test".into(),
            source: "alice".into(),
            target: "bob".into(),
            relation: "knows".into(),
            weight: 0.5,
            memory_ids: vec![],
        })
        .unwrap();

    let id2 = mast
        .relate(RelateRequest {
            collection: "test".into(),
            source: "alice".into(),
            target: "bob".into(),
            relation: "knows".into(),
            weight: 0.9,
            memory_ids: vec![],
        })
        .unwrap();

    // Should reuse same edge ID
    assert_eq!(id1, id2);

    // Verify weight was updated
    let graph = mast.subgraph("test").unwrap();
    assert_eq!(graph.edges.len(), 1);
    assert!((graph.edges[0].weight - 0.9).abs() < 0.01);
}

#[tokio::test]
async fn unrelate() {
    let (_dir, config) = temp_config();
    let mast = Mast::open(config).unwrap();

    mast.relate(RelateRequest {
        collection: "test".into(),
        source: "alice".into(),
        target: "bob".into(),
        relation: "knows".into(),
        weight: 0.9,
        memory_ids: vec![],
    })
    .unwrap();

    mast.unrelate("test", "alice", "bob", "knows").unwrap();

    let graph = mast.subgraph("test").unwrap();
    assert!(graph.edges.is_empty());
}

#[tokio::test]
async fn traverse_depth_limit() {
    let (_dir, config) = temp_config();
    let mast = Mast::open(config).unwrap();

    mast.relate(RelateRequest {
        collection: "test".into(),
        source: "a".into(),
        target: "b".into(),
        relation: "r".into(),
        weight: 1.0,
        memory_ids: vec![],
    })
    .unwrap();

    mast.relate(RelateRequest {
        collection: "test".into(),
        source: "b".into(),
        target: "c".into(),
        relation: "r".into(),
        weight: 1.0,
        memory_ids: vec![],
    })
    .unwrap();

    mast.relate(RelateRequest {
        collection: "test".into(),
        source: "c".into(),
        target: "d".into(),
        relation: "r".into(),
        weight: 1.0,
        memory_ids: vec![],
    })
    .unwrap();

    // Depth 1 should only reach b
    let edges = mast
        .traverse(TraverseRequest {
            collection: "test".into(),
            start: "a".into(),
            max_depth: 1,
            relation_filter: None,
            min_weight: None,
        })
        .unwrap();

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].target, "b");
}

#[tokio::test]
async fn traverse_relation_filter() {
    let (_dir, config) = temp_config();
    let mast = Mast::open(config).unwrap();

    mast.relate(RelateRequest {
        collection: "test".into(),
        source: "a".into(),
        target: "b".into(),
        relation: "knows".into(),
        weight: 1.0,
        memory_ids: vec![],
    })
    .unwrap();

    mast.relate(RelateRequest {
        collection: "test".into(),
        source: "a".into(),
        target: "c".into(),
        relation: "likes".into(),
        weight: 1.0,
        memory_ids: vec![],
    })
    .unwrap();

    let edges = mast
        .traverse(TraverseRequest {
            collection: "test".into(),
            start: "a".into(),
            max_depth: 3,
            relation_filter: Some("knows".into()),
            min_weight: None,
        })
        .unwrap();

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].relation, "knows");
}

#[tokio::test]
async fn traverse_min_weight() {
    let (_dir, config) = temp_config();
    let mast = Mast::open(config).unwrap();

    mast.relate(RelateRequest {
        collection: "test".into(),
        source: "a".into(),
        target: "b".into(),
        relation: "r".into(),
        weight: 0.3,
        memory_ids: vec![],
    })
    .unwrap();

    mast.relate(RelateRequest {
        collection: "test".into(),
        source: "a".into(),
        target: "c".into(),
        relation: "r".into(),
        weight: 0.8,
        memory_ids: vec![],
    })
    .unwrap();

    let edges = mast
        .traverse(TraverseRequest {
            collection: "test".into(),
            start: "a".into(),
            max_depth: 3,
            relation_filter: None,
            min_weight: Some(0.5),
        })
        .unwrap();

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].target, "c");
}

#[tokio::test]
async fn subgraph() {
    let (_dir, config) = temp_config();
    let mast = Mast::open(config).unwrap();

    mast.relate(RelateRequest {
        collection: "test".into(),
        source: "a".into(),
        target: "b".into(),
        relation: "r".into(),
        weight: 1.0,
        memory_ids: vec![],
    })
    .unwrap();

    mast.relate(RelateRequest {
        collection: "test".into(),
        source: "b".into(),
        target: "c".into(),
        relation: "r".into(),
        weight: 1.0,
        memory_ids: vec![],
    })
    .unwrap();

    let graph = mast.subgraph("test").unwrap();
    assert_eq!(graph.collection, "test");
    assert_eq!(graph.edges.len(), 2);
}

// ---- Batch store tests ----

#[tokio::test]
async fn batch_store() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    let memories = mast
        .store_batch(
            BatchStoreRequest {
                collection: "test".into(),
                items: vec![
                    BatchStoreItem {
                        content: "first".into(),
                        embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                        metadata: HashMap::new(),
                        tier: Tier::Active,
                        ttl: None,
                    },
                    BatchStoreItem {
                        content: "second".into(),
                        embedding: Some(vec![0.0, 1.0, 0.0, 0.0]),
                        metadata: HashMap::new(),
                        tier: Tier::Active,
                        ttl: None,
                    },
                    BatchStoreItem {
                        content: "third".into(),
                        embedding: None, // needs embedding
                        metadata: HashMap::from([("k".into(), "v".into())]),
                        tier: Tier::Core,
                        ttl: Some(3600),
                    },
                ],
            },
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(memories.len(), 3);
    assert_eq!(memories[0].content, "first");
    assert_eq!(memories[2].tier, Tier::Core);
    assert_eq!(memories[2].ttl, Some(3600));

    // All should be retrievable
    for mem in &memories {
        let found = mast.get("test", &mem.id).unwrap();
        assert!(found.is_some());
    }

    let info = mast.info().unwrap();
    assert_eq!(info[0].memory_count, 3);
}

#[tokio::test]
async fn batch_store_recall() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    mast.store_batch(
        BatchStoreRequest {
            collection: "test".into(),
            items: vec![
                BatchStoreItem {
                    content: "target".into(),
                    embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                    metadata: HashMap::new(),
                    tier: Tier::Active,
                    ttl: None,
                },
                BatchStoreItem {
                    content: "other".into(),
                    embedding: Some(vec![0.0, 0.0, 0.0, 1.0]),
                    metadata: HashMap::new(),
                    tier: Tier::Active,
                    ttl: None,
                },
            ],
        },
        &embedder,
    )
    .await
    .unwrap();

    let results = mast
        .recall(
            RecallRequest {
                collection: "test".into(),
                query: None,
                query_embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 1,
                filter: None,
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].memory.content, "target");
}

// ---- Versioned serde tests ----

#[tokio::test]
async fn versioned_serde_roundtrip() {
    use mast_core::storage::serde::{deserialize_memory, serialize_memory};
    use mast_core::types::Memory;

    let mem = Memory {
        id: ulid::Ulid::new(),
        collection: "test".into(),
        content: "versioned".into(),
        embedding: vec![0.1, 0.2],
        metadata: HashMap::new(),
        tier: Tier::Active,
        created_at: 1000,
        access_count: 0,
        last_accessed: 1000,
        ttl: Some(3600),
        compacted_at: None,
        parent_ids: vec![ulid::Ulid::new()],
    };

    let data = serialize_memory(&mem).unwrap();
    let decoded = deserialize_memory(&data).unwrap();
    assert_eq!(decoded.id, mem.id);
    assert_eq!(decoded.ttl, Some(3600));
    assert_eq!(decoded.parent_ids.len(), 1);
}

#[tokio::test]
async fn v0_data_compat() {
    use mast_core::storage::serde::deserialize_memory;

    // Simulate v0 data: raw bincode of old Memory layout (no version byte prefix)
    #[derive(serde::Serialize)]
    struct OldMemory {
        id: ulid::Ulid,
        collection: String,
        content: String,
        embedding: Vec<f32>,
        metadata: HashMap<String, String>,
        tier: Tier,
        created_at: i64,
        access_count: u64,
        last_accessed: i64,
    }

    let old = OldMemory {
        id: ulid::Ulid::new(),
        collection: "legacy".into(),
        content: "old format".into(),
        embedding: vec![1.0, 2.0],
        metadata: HashMap::new(),
        tier: Tier::Core,
        created_at: 500,
        access_count: 10,
        last_accessed: 600,
    };

    let data = bincode::serialize(&old).unwrap();
    let decoded = deserialize_memory(&data).unwrap();

    assert_eq!(decoded.id, old.id);
    assert_eq!(decoded.content, "old format");
    assert_eq!(decoded.ttl, None);
    assert_eq!(decoded.compacted_at, None);
    assert!(decoded.parent_ids.is_empty());
}

// ---- Regression tests for bug fixes ----

#[tokio::test]
async fn compact_zero_batch_size_returns_error() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let compactor = MockCompactor;
    let mut mast = Mast::open(config).unwrap();

    let m1 = mast
        .store(
            StoreRequest {
                collection: "test".into(),
                content: "alpha".into(),
                embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                metadata: HashMap::new(),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    let result = mast
        .compact(
            CompactRequest {
                collection: "test".into(),
                source_tier: Tier::Active,
                target_tier: Tier::Archive,
                strategy: CompactionStrategy::Manual {
                    ids: vec![m1.id],
                },
                max_batch_size: 0,
                dry_run: false,
            },
            &compactor,
            &embedder,
        )
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("max_batch_size"),
        "expected max_batch_size error, got: {err}"
    );
}

#[tokio::test]
async fn store_batch_short_embedder_response_returns_error() {
    use mast_core::error::MastError;

    struct ShortEmbedder;

    #[async_trait::async_trait]
    impl mast_core::embed::Embedder for ShortEmbedder {
        async fn embed(&self, _text: &str) -> Result<Vec<f32>, MastError> {
            Ok(vec![1.0, 0.0, 0.0, 0.0])
        }
        async fn embed_batch(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, MastError> {
            // Return fewer vectors than requested
            Ok(vec![vec![1.0, 0.0, 0.0, 0.0]])
        }
        fn dimensions(&self) -> usize {
            4
        }
    }

    let (_dir, config) = temp_config();
    let mut mast = Mast::open(config).unwrap();

    let result = mast
        .store_batch(
            BatchStoreRequest {
                collection: "test".into(),
                items: vec![
                    BatchStoreItem {
                        content: "first".into(),
                        embedding: None,
                        metadata: HashMap::new(),
                        tier: Tier::Active,
                        ttl: None,
                    },
                    BatchStoreItem {
                        content: "second".into(),
                        embedding: None,
                        metadata: HashMap::new(),
                        tier: Tier::Active,
                        ttl: None,
                    },
                ],
            },
            &ShortEmbedder,
        )
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("returned 1 vectors, expected 2"),
        "expected length mismatch error, got: {err}"
    );
}

/// Helper: create a storage + poisoned vector index.
///
/// Writes `num_real` memories (seq 0..N-1) and a vector index that contains
/// keys 0..N-1 plus the extra `poison_key`. When Mast opens this DB it will
/// load the poisoned index; any write that gets `poison_key` as its seq will
/// hit usearch's "duplicate key" rejection.
fn setup_poisoned_db(
    db_path: &std::path::Path,
    collection: &str,
    dims: usize,
    num_real: usize,
    poison_key: u64,
) -> Vec<Memory> {
    use mast_core::index::vector::VectorIndex;
    use mast_core::storage::Storage;

    let storage = Storage::open(db_path).unwrap();
    let index = VectorIndex::new(dims).unwrap();

    let mut memories = Vec::new();
    for i in 0..num_real {
        let mut emb = vec![0.0f32; dims];
        emb[i % dims] = 1.0;
        let mem = Memory {
            id: ulid::Ulid::new(),
            collection: collection.into(),
            content: format!("seed_{i}"),
            embedding: emb.clone(),
            metadata: HashMap::new(),
            tier: Tier::Active,
            created_at: 1000,
            access_count: 0,
            last_accessed: 1000,
            ttl: None,
            compacted_at: None,
            parent_ids: vec![],
        };
        let seq = storage.write_memory(&mem).unwrap();
        index.add(seq, &emb).unwrap();
        memories.push(mem);
    }

    // Poison: insert a key that the next write(s) will collide with
    let mut poison_emb = vec![0.0f32; dims];
    poison_emb[0] = 0.42;
    index.add(poison_key, &poison_emb).unwrap();

    // Persist the poisoned index
    let buffer = index.to_buffer().unwrap();
    storage.write_vector_index(collection, &buffer).unwrap();

    memories
}

#[tokio::test]
async fn store_rollback_on_index_failure() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.mast");
    let embedder = MockEmbedder::new(4);

    // Write 1 real memory (seq 0). Poison key 1 so next store fails.
    let originals = setup_poisoned_db(&db_path, "test", 4, 1, 1);
    assert_eq!(originals.len(), 1);

    let config = MastConfig::default().with_db_path(&db_path);
    let mut mast = Mast::open(config).unwrap();

    // This store will get seq 1 → index.add(1, ...) hits duplicate key
    let result = mast
        .store(
            StoreRequest {
                collection: "test".into(),
                content: "should be rolled back".into(),
                embedding: Some(vec![0.0, 1.0, 0.0, 0.0]),
                metadata: HashMap::from([("k".into(), "v".into())]),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await;

    assert!(result.is_err(), "store should fail on duplicate key");

    // Rollback check: storage should contain only the original memory
    let remaining = mast.list("test").unwrap();
    assert_eq!(
        remaining.len(),
        1,
        "expected 1 memory after rollback, got {}",
        remaining.len()
    );
    assert_eq!(remaining[0].id, originals[0].id);

    assert_collection_count(&mast, "test", 1);
}

#[tokio::test]
async fn store_batch_rollback_on_index_failure() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.mast");
    let embedder = MockEmbedder::new(4);

    // Write 1 real memory (seq 0). Poison key 2 so the second batch item fails.
    // Batch of 2: first gets seq 1 (ok), second gets seq 2 (duplicate → error).
    let originals = setup_poisoned_db(&db_path, "test", 4, 1, 2);

    let config = MastConfig::default().with_db_path(&db_path);
    let mut mast = Mast::open(config).unwrap();

    let result = mast
        .store_batch(
            BatchStoreRequest {
                collection: "test".into(),
                items: vec![
                    BatchStoreItem {
                        content: "batch_a".into(),
                        embedding: Some(vec![0.0, 1.0, 0.0, 0.0]),
                        metadata: HashMap::from([("tag".into(), "a".into())]),
                        tier: Tier::Active,
                        ttl: None,
                    },
                    BatchStoreItem {
                        content: "batch_b".into(),
                        embedding: Some(vec![0.0, 0.0, 1.0, 0.0]),
                        metadata: HashMap::from([("tag".into(), "b".into())]),
                        tier: Tier::Active,
                        ttl: None,
                    },
                ],
            },
            &embedder,
        )
        .await;

    assert!(result.is_err(), "store_batch should fail on duplicate key");

    // Rollback check: only the original seed memory should remain
    let remaining = mast.list("test").unwrap();
    assert_eq!(
        remaining.len(),
        1,
        "expected 1 memory after batch rollback, got {}",
        remaining.len()
    );
    assert_eq!(remaining[0].id, originals[0].id);

    assert_collection_count(&mast, "test", 1);
}

#[tokio::test]
async fn compact_rollback_on_index_failure() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.mast");
    let embedder = MockEmbedder::new(4);
    let compactor = MockCompactor;

    // Write 2 real memories (seq 0, 1). Poison key 2 so compact's new memory fails.
    let originals = setup_poisoned_db(&db_path, "test", 4, 2, 2);

    let config = MastConfig::default().with_db_path(&db_path);
    let mut mast = Mast::open(config).unwrap();

    let result = mast
        .compact(
            CompactRequest {
                collection: "test".into(),
                source_tier: Tier::Active,
                target_tier: Tier::Archive,
                strategy: CompactionStrategy::Manual {
                    ids: originals.iter().map(|m| m.id).collect(),
                },
                max_batch_size: 10,
                dry_run: false,
            },
            &compactor,
            &embedder,
        )
        .await;

    assert!(result.is_err(), "compact should fail on duplicate key");

    // Rollback check: new compacted memory should not exist in storage
    let all = mast.list("test").unwrap();
    assert_eq!(
        all.len(),
        2,
        "expected 2 original memories after compact rollback, got {}",
        all.len()
    );

    // Source memories should NOT be marked compacted (rollback happened before marking)
    for mem in &all {
        assert!(
            mem.compacted_at.is_none(),
            "source memory {} should not be marked compacted after rollback",
            mem.id
        );
    }

    assert_collection_count(&mast, "test", 2);
}

// ---- Fulltext / Hybrid / Snapshot tests ----

#[tokio::test]
async fn recall_fulltext_basic() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(32);
    let mut mast = Mast::open(config).unwrap();

    // Store memories with distinct text content
    for content in &[
        "rust programming language systems",
        "python scripting language data",
        "javascript web browser programming",
    ] {
        mast.store(StoreRequest {
            collection: "test".into(),
            content: content.to_string(),
            embedding: None,
            metadata: HashMap::new(),
            tier: Tier::Active,
            ttl: None,
        }, &embedder).await.unwrap();
    }

    let results = mast.recall(RecallRequest {
        collection: "test".into(),
        query: None,
        query_embedding: None,
        text_query: Some("rust programming".into()),
        search_mode: SearchMode::FullText,
        limit: 10,
        filter: None,
        tier_weights: None,
        time_decay: None,
    }, &embedder).await.unwrap();

    assert!(!results.is_empty());
    assert!(results[0].memory.content.contains("rust"));
}

#[tokio::test]
async fn recall_hybrid_basic() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(32);
    let mut mast = Mast::open(config).unwrap();

    for content in &[
        "rust programming language systems",
        "python scripting language data",
        "javascript web browser programming",
    ] {
        mast.store(StoreRequest {
            collection: "test".into(),
            content: content.to_string(),
            embedding: None,
            metadata: HashMap::new(),
            tier: Tier::Active,
            ttl: None,
        }, &embedder).await.unwrap();
    }

    let results = mast.recall(RecallRequest {
        collection: "test".into(),
        query: Some("programming".into()),
        query_embedding: None,
        text_query: Some("rust programming".into()),
        search_mode: SearchMode::Hybrid { vector_weight: 0.5, text_weight: 0.5 },
        limit: 10,
        filter: None,
        tier_weights: None,
        time_decay: None,
    }, &embedder).await.unwrap();

    assert!(!results.is_empty());
}

#[tokio::test]
async fn snapshot_and_restore_integration() {
    use std::io::Cursor;

    let (_dir1, config1) = temp_config();
    let embedder = MockEmbedder::new(32);
    let mut mast1 = Mast::open(config1).unwrap();

    for i in 0..5 {
        mast1.store(StoreRequest {
            collection: "snap".into(),
            content: format!("snapshot memory {i}"),
            embedding: None,
            metadata: HashMap::from([("idx".into(), i.to_string())]),
            tier: Tier::Active,
            ttl: None,
        }, &embedder).await.unwrap();
    }

    mast1.relate(RelateRequest {
        collection: "snap".into(),
        source: "a".into(),
        target: "b".into(),
        relation: "linked".into(),
        weight: 1.0,
        memory_ids: vec![],
    }).unwrap();

    let mut buf = Vec::new();
    let snap = mast1.snapshot("snap", &mut buf).unwrap();
    assert_eq!(snap.memory_count, 5);
    assert_eq!(snap.edge_count, 1);
    drop(mast1);

    let (_dir2, config2) = temp_config();
    let mut mast2 = Mast::open(config2).unwrap();
    let mut cursor = Cursor::new(&buf);
    let restore = mast2.restore(&mut cursor, false).unwrap();
    assert_eq!(restore.memory_count, 5);
    assert_eq!(restore.edge_count, 1);

    let memories = mast2.list("snap").unwrap();
    assert_eq!(memories.len(), 5);

    let graph = mast2.subgraph("snap").unwrap();
    assert_eq!(graph.edges.len(), 1);

    // Verify vector recall works after restore
    let results = mast2.recall(RecallRequest {
        collection: "snap".into(),
        query: Some("snapshot".into()),
        query_embedding: None,
        text_query: None,
        search_mode: SearchMode::Vector,
        limit: 10,
        filter: None,
        tier_weights: None,
        time_decay: None,
    }, &embedder).await.unwrap();
    assert_eq!(results.len(), 5);

    // Verify fulltext search works after restore
    let ft_results = mast2.recall(RecallRequest {
        collection: "snap".into(),
        query: None,
        query_embedding: None,
        text_query: Some("snapshot memory".into()),
        search_mode: SearchMode::FullText,
        limit: 10,
        filter: None,
        tier_weights: None,
        time_decay: None,
    }, &embedder).await.unwrap();
    assert_eq!(ft_results.len(), 5);
}

// ---- Or metadata filter tests ----

#[tokio::test]
async fn metadata_filter_or_through_recall() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    for (content, color) in [("a", "red"), ("b", "blue"), ("c", "green")] {
        mast.store(
            StoreRequest {
                collection: "test".into(),
                content: content.into(),
                embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                metadata: HashMap::from([("color".into(), color.into())]),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await
        .unwrap();
    }

    let results = mast
        .recall(
            RecallRequest {
                collection: "test".into(),
                query: None,
                query_embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 10,
                filter: Some(MetadataFilter::Or(vec![
                    MetadataFilter::Eq("color".into(), "red".into()),
                    MetadataFilter::Eq("color".into(), "green".into()),
                ])),
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 2);
    let contents: Vec<&str> = results.iter().map(|r| r.memory.content.as_str()).collect();
    assert!(contents.contains(&"a"));
    assert!(contents.contains(&"c"));
    mast.close().unwrap();
}

#[tokio::test]
async fn metadata_filter_nested_and_or() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    // red+circle, red+square, blue+circle, green+circle
    for (content, color, shape) in [
        ("red_circle", "red", "circle"),
        ("red_square", "red", "square"),
        ("blue_circle", "blue", "circle"),
        ("green_circle", "green", "circle"),
    ] {
        mast.store(
            StoreRequest {
                collection: "test".into(),
                content: content.into(),
                embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                metadata: HashMap::from([
                    ("color".into(), color.into()),
                    ("shape".into(), shape.into()),
                ]),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await
        .unwrap();
    }

    // And([Or([Eq(color,red), Eq(color,blue)]), Eq(shape,circle)])
    let results = mast
        .recall(
            RecallRequest {
                collection: "test".into(),
                query: None,
                query_embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 10,
                filter: Some(MetadataFilter::And(vec![
                    MetadataFilter::Or(vec![
                        MetadataFilter::Eq("color".into(), "red".into()),
                        MetadataFilter::Eq("color".into(), "blue".into()),
                    ]),
                    MetadataFilter::Eq("shape".into(), "circle".into()),
                ])),
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 2);
    let contents: Vec<&str> = results.iter().map(|r| r.memory.content.as_str()).collect();
    assert!(contents.contains(&"red_circle"));
    assert!(contents.contains(&"blue_circle"));
    mast.close().unwrap();
}

#[tokio::test]
async fn metadata_filter_or_matches_nothing() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    mast.store(
        StoreRequest {
            collection: "test".into(),
            content: "exists".into(),
            embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
            metadata: HashMap::from([("color".into(), "red".into())]),
            tier: Tier::Active,
            ttl: None,
        },
        &embedder,
    )
    .await
    .unwrap();

    let results = mast
        .recall(
            RecallRequest {
                collection: "test".into(),
                query: None,
                query_embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 10,
                filter: Some(MetadataFilter::Or(vec![
                    MetadataFilter::Eq("color".into(), "purple".into()),
                    MetadataFilter::Eq("color".into(), "orange".into()),
                ])),
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert!(results.is_empty());
    mast.close().unwrap();
}

// ---- Hybrid search normalization tests ----

#[tokio::test]
async fn recall_hybrid_vector_only_no_ft_matches() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    mast.store(
        StoreRequest {
            collection: "test".into(),
            content: "alpha bravo".into(),
            embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
            metadata: HashMap::new(),
            tier: Tier::Active,
            ttl: None,
        },
        &embedder,
    )
    .await
    .unwrap();

    // text_query "zzzznonexistent" won't match BM25, but vector should still return results
    let results = mast
        .recall(
            RecallRequest {
                collection: "test".into(),
                query: None,
                query_embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                text_query: Some("zzzznonexistent".into()),
                search_mode: SearchMode::Hybrid {
                    vector_weight: 0.5,
                    text_weight: 0.5,
                },
                limit: 10,
                filter: None,
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].memory.content, "alpha bravo");
    mast.close().unwrap();
}

#[tokio::test]
async fn recall_hybrid_ft_only_no_vector_matches() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    mast.store(
        StoreRequest {
            collection: "test".into(),
            content: "unique keyword searchable".into(),
            embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
            metadata: HashMap::new(),
            tier: Tier::Active,
            ttl: None,
        },
        &embedder,
    )
    .await
    .unwrap();

    // query_embedding orthogonal to stored vector, but text_query matches BM25
    let results = mast
        .recall(
            RecallRequest {
                collection: "test".into(),
                query: None,
                query_embedding: Some(vec![0.0, 0.0, 0.0, 1.0]),
                text_query: Some("unique keyword".into()),
                search_mode: SearchMode::Hybrid {
                    vector_weight: 0.5,
                    text_weight: 0.5,
                },
                limit: 10,
                filter: None,
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert!(!results.is_empty());
    assert_eq!(results[0].memory.content, "unique keyword searchable");
    mast.close().unwrap();
}

#[tokio::test]
async fn recall_hybrid_both_empty() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    // Empty collection — both vector and fulltext return nothing
    let results = mast
        .recall(
            RecallRequest {
                collection: "empty".into(),
                query: None,
                query_embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                text_query: Some("anything".into()),
                search_mode: SearchMode::Hybrid {
                    vector_weight: 0.5,
                    text_weight: 0.5,
                },
                limit: 10,
                filter: None,
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert!(results.is_empty());
}

#[tokio::test]
async fn recall_hybrid_single_result_normalization() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let mut mast = Mast::open(config).unwrap();

    mast.store(
        StoreRequest {
            collection: "test".into(),
            content: "solo target memory".into(),
            embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
            metadata: HashMap::new(),
            tier: Tier::Active,
            ttl: None,
        },
        &embedder,
    )
    .await
    .unwrap();

    // Single result: normalize(x, x, x) -> 1.0 (constant score path)
    let results = mast
        .recall(
            RecallRequest {
                collection: "test".into(),
                query: None,
                query_embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                text_query: Some("solo target".into()),
                search_mode: SearchMode::Hybrid {
                    vector_weight: 0.6,
                    text_weight: 0.4,
                },
                limit: 10,
                filter: None,
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    // With single result, normalize returns 1.0 for both modes
    // score = vector_weight * 1.0 + text_weight * 1.0 = 0.6 + 0.4 = 1.0
    assert!(
        results[0].score > 0.0,
        "expected positive score, got {}",
        results[0].score
    );
    mast.close().unwrap();
}

// ---- Compaction strategy tests ----

#[tokio::test]
async fn compact_age_strategy() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let compactor = MockCompactor;
    let mut mast = Mast::open(config).unwrap();

    mast.store(
        StoreRequest {
            collection: "test".into(),
            content: "old enough".into(),
            embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
            metadata: HashMap::new(),
            tier: Tier::Active,
            ttl: None,
        },
        &embedder,
    )
    .await
    .unwrap();

    mast.store(
        StoreRequest {
            collection: "test".into(),
            content: "also old enough".into(),
            embedding: Some(vec![0.0, 1.0, 0.0, 0.0]),
            metadata: HashMap::new(),
            tier: Tier::Active,
            ttl: None,
        },
        &embedder,
    )
    .await
    .unwrap();

    // Sleep briefly so now - created_at > 0
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

    // older_than_secs: 0 means anything > 0 seconds old matches
    let result = mast
        .compact(
            CompactRequest {
                collection: "test".into(),
                source_tier: Tier::Active,
                target_tier: Tier::Archive,
                strategy: CompactionStrategy::Age { older_than_secs: 0 },
                max_batch_size: 10,
                dry_run: false,
            },
            &compactor,
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(result.source_count, 2);
    assert_eq!(result.compacted_count, 2);
    assert_eq!(result.new_memory_ids.len(), 1);
    mast.close().unwrap();
}

#[tokio::test]
async fn compact_age_strategy_none_old_enough() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let compactor = MockCompactor;
    let mut mast = Mast::open(config).unwrap();

    mast.store(
        StoreRequest {
            collection: "test".into(),
            content: "fresh".into(),
            embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
            metadata: HashMap::new(),
            tier: Tier::Active,
            ttl: None,
        },
        &embedder,
    )
    .await
    .unwrap();

    // older_than_secs very large — nothing is old enough
    let result = mast
        .compact(
            CompactRequest {
                collection: "test".into(),
                source_tier: Tier::Active,
                target_tier: Tier::Archive,
                strategy: CompactionStrategy::Age {
                    older_than_secs: 999_999,
                },
                max_batch_size: 10,
                dry_run: false,
            },
            &compactor,
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(result.source_count, 0);
    assert_eq!(result.compacted_count, 0);
    assert!(result.new_memory_ids.is_empty());
    mast.close().unwrap();
}

#[tokio::test]
async fn compact_access_decay_strategy() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("access_decay.mast");
    let embedder = MockEmbedder::new(4);
    let compactor = MockCompactor;

    let m1_id;
    {
        let config = MastConfig::default().with_db_path(&db_path);
        let mut mast = Mast::open(config).unwrap();

        let m1 = mast
            .store(
                StoreRequest {
                    collection: "test".into(),
                    content: "will be accessed".into(),
                    embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                    metadata: HashMap::new(),
                    tier: Tier::Active,
                    ttl: None,
                },
                &embedder,
            )
            .await
            .unwrap();
        m1_id = m1.id;

        mast.store(
            StoreRequest {
                collection: "test".into(),
                content: "never accessed".into(),
                embedding: Some(vec![0.0, 1.0, 0.0, 0.0]),
                metadata: HashMap::new(),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await
        .unwrap();

        // Recall to bump access_count on m1
        mast.recall(
            RecallRequest {
                collection: "test".into(),
                query: None,
                query_embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 1,
                filter: None,
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await
        .unwrap();

        // close flushes access stats
        mast.close().unwrap();
    }

    // Reopen to pick up flushed access stats
    let config2 = MastConfig::default().with_db_path(&db_path);
    let mut mast = Mast::open(config2).unwrap();

    // Verify m1 has access_count > 0
    let m1_after = mast.get("test", &m1_id).unwrap().unwrap();
    assert!(
        m1_after.access_count > 0,
        "expected access_count > 0, got {}",
        m1_after.access_count
    );

    // AccessDecay: only memories with access_count == 0 are candidates
    let result = mast
        .compact(
            CompactRequest {
                collection: "test".into(),
                source_tier: Tier::Active,
                target_tier: Tier::Archive,
                strategy: CompactionStrategy::AccessDecay,
                max_batch_size: 10,
                dry_run: false,
            },
            &compactor,
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(result.source_count, 1, "only unaccessed memory should be a candidate");
    assert_eq!(result.compacted_count, 1);
    mast.close().unwrap();
}

#[tokio::test]
async fn compact_hybrid_strategy_fresh_not_matched() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let compactor = MockCompactor;
    let mut mast = Mast::open(config).unwrap();

    // Store a fresh memory with access_count=0
    // Hybrid requires: now - created_at > 86400 AND access_count < 3
    // Fresh memory won't satisfy the 86400s age threshold
    mast.store(
        StoreRequest {
            collection: "test".into(),
            content: "fresh and unaccessed".into(),
            embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
            metadata: HashMap::new(),
            tier: Tier::Active,
            ttl: None,
        },
        &embedder,
    )
    .await
    .unwrap();

    let result = mast
        .compact(
            CompactRequest {
                collection: "test".into(),
                source_tier: Tier::Active,
                target_tier: Tier::Archive,
                strategy: CompactionStrategy::Hybrid,
                max_batch_size: 10,
                dry_run: false,
            },
            &compactor,
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(result.source_count, 0, "fresh memory should not match Hybrid strategy");
    assert_eq!(result.compacted_count, 0);
    assert!(result.new_memory_ids.is_empty());
    mast.close().unwrap();
}

#[tokio::test]
async fn compact_strategy_skips_wrong_tier() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(4);
    let compactor = MockCompactor;
    let mut mast = Mast::open(config).unwrap();

    mast.store(
        StoreRequest {
            collection: "test".into(),
            content: "core memory".into(),
            embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
            metadata: HashMap::new(),
            tier: Tier::Core,
            ttl: None,
        },
        &embedder,
    )
    .await
    .unwrap();

    // Compact with source_tier: Active, but memory is Core
    let result = mast
        .compact(
            CompactRequest {
                collection: "test".into(),
                source_tier: Tier::Active,
                target_tier: Tier::Archive,
                strategy: CompactionStrategy::Age { older_than_secs: 0 },
                max_batch_size: 10,
                dry_run: false,
            },
            &compactor,
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(result.source_count, 0, "Core memory should not be selected when source_tier is Active");
    assert_eq!(result.compacted_count, 0);
    assert!(result.new_memory_ids.is_empty());
    mast.close().unwrap();
}
