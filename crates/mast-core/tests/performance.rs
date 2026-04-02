use mast_core::config::MastConfig;
use mast_core::embed::MockEmbedder;
use mast_core::types::*;
use mast_core::Mast;
use std::collections::HashMap;
use std::time::Instant;

fn temp_config() -> (tempfile::TempDir, MastConfig) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("perf.mast");
    let config = MastConfig::default().with_db_path(db_path);
    (dir, config)
}

// --- Store throughput ---

#[tokio::test]
async fn store_single_throughput() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(128);
    let mut mast = Mast::open(config).unwrap();

    let n = 100;
    let start = Instant::now();
    for i in 0..n {
        mast.store(
            StoreRequest {
                collection: "perf".into(),
                content: format!("performance test document number {i} with some content"),
                embedding: None,
                metadata: HashMap::from([("idx".into(), i.to_string())]),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await
        .unwrap();
    }
    let elapsed = start.elapsed();
    let per_op = elapsed / n;

    eprintln!(
        "store_single: {n} ops in {elapsed:?} ({per_op:?}/op, {:.0} ops/s)",
        n as f64 / elapsed.as_secs_f64()
    );

    // Each store should complete in <20ms (regression threshold).
    // The pre-fix version took ~120ms/op; consolidated txn should be <5ms.
    assert!(
        per_op.as_millis() < 20,
        "store too slow: {per_op:?}/op (limit: 20ms)"
    );

    mast.close().unwrap();
}

#[tokio::test]
async fn store_single_with_metadata_throughput() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(128);
    let mut mast = Mast::open(config).unwrap();

    let n = 100;
    let start = Instant::now();
    for i in 0..n {
        mast.store(
            StoreRequest {
                collection: "perf".into(),
                content: format!("doc {i} about testing performance of metadata-heavy stores"),
                embedding: None,
                metadata: HashMap::from([
                    ("author".into(), "bench".into()),
                    ("category".into(), format!("cat_{}", i % 10)),
                    ("priority".into(), (i % 5).to_string()),
                    ("source".into(), "perf_test".into()),
                ]),
                tier: Tier::Active,
                ttl: None,
            },
            &embedder,
        )
        .await
        .unwrap();
    }
    let elapsed = start.elapsed();
    let per_op = elapsed / n;

    eprintln!(
        "store_with_metadata: {n} ops in {elapsed:?} ({per_op:?}/op, {:.0} ops/s)",
        n as f64 / elapsed.as_secs_f64()
    );

    assert!(
        per_op.as_millis() < 25,
        "store with metadata too slow: {per_op:?}/op (limit: 25ms)"
    );

    mast.close().unwrap();
}

// --- Batch store throughput ---

#[tokio::test]
async fn store_batch_100_throughput() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(128);
    let mut mast = Mast::open(config).unwrap();

    let items: Vec<BatchStoreItem> = (0..100)
        .map(|i| BatchStoreItem {
            content: format!("batch perf item {i} with text for fulltext indexing"),
            embedding: None,
            metadata: HashMap::from([("idx".into(), i.to_string())]),
            tier: Tier::Active,
            ttl: None,
        })
        .collect();

    let start = Instant::now();
    mast.store_batch(
        BatchStoreRequest {
            collection: "perf".into(),
            items,
        },
        &embedder,
    )
    .await
    .unwrap();
    let elapsed = start.elapsed();

    eprintln!(
        "store_batch_100: {elapsed:?} ({:.0} items/s)",
        100.0 / elapsed.as_secs_f64()
    );

    // 100-item batch should complete in <500ms.
    // With single-txn optimization this should be well under 100ms.
    assert!(
        elapsed.as_millis() < 500,
        "batch store too slow: {elapsed:?} (limit: 500ms)"
    );

    mast.close().unwrap();
}

#[tokio::test]
async fn store_batch_1000_throughput() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(128);
    let mut mast = Mast::open(config).unwrap();

    let items: Vec<BatchStoreItem> = (0..1000)
        .map(|i| BatchStoreItem {
            content: format!("large batch item {i} for throughput measurement"),
            embedding: None,
            metadata: HashMap::from([("idx".into(), i.to_string())]),
            tier: Tier::Active,
            ttl: None,
        })
        .collect();

    let start = Instant::now();
    mast.store_batch(
        BatchStoreRequest {
            collection: "perf".into(),
            items,
        },
        &embedder,
    )
    .await
    .unwrap();
    let elapsed = start.elapsed();

    eprintln!(
        "store_batch_1000: {elapsed:?} ({:.0} items/s)",
        1000.0 / elapsed.as_secs_f64()
    );

    // 1000 items should complete in <5s.
    assert!(
        elapsed.as_secs() < 5,
        "batch store 1000 too slow: {elapsed:?} (limit: 5s)"
    );

    mast.close().unwrap();
}

// --- Recall latency ---

async fn seed(mast: &mut Mast, embedder: &MockEmbedder, collection: &str, count: usize) {
    let items: Vec<BatchStoreItem> = (0..count)
        .map(|i| BatchStoreItem {
            content: format!("seeded memory {i} about various topics including performance"),
            embedding: None,
            metadata: HashMap::from([
                ("idx".into(), i.to_string()),
                ("group".into(), (i % 5).to_string()),
            ]),
            tier: Tier::Active,
            ttl: None,
        })
        .collect();

    for chunk in items.chunks(500) {
        mast.store_batch(
            BatchStoreRequest {
                collection: collection.into(),
                items: chunk.to_vec(),
            },
            embedder,
        )
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn recall_vector_latency() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(128);
    let mut mast = Mast::open(config).unwrap();
    seed(&mut mast, &embedder, "perf", 1000).await;

    let n = 50u32;
    let start = Instant::now();
    for _ in 0..n {
        mast.recall(
            RecallRequest {
                collection: "perf".into(),
                query: Some("performance topics".into()),
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
        .await
        .unwrap();
    }
    let elapsed = start.elapsed();
    let per_op = elapsed / n;

    eprintln!(
        "recall_vector (n=1000): {n} queries in {elapsed:?} ({per_op:?}/query)"
    );

    // Deferred access stats: was ~19ms, now ~2-3ms
    assert!(
        per_op.as_millis() < 10,
        "vector recall too slow: {per_op:?}/query (limit: 10ms)"
    );

    mast.close().unwrap();
}

#[tokio::test]
async fn recall_fulltext_latency() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(128);
    let mut mast = Mast::open(config).unwrap();
    seed(&mut mast, &embedder, "perf", 1000).await;

    let n = 50u32;
    let start = Instant::now();
    for _ in 0..n {
        mast.recall(
            RecallRequest {
                collection: "perf".into(),
                query: None,
                query_embedding: None,
                text_query: Some("performance topics memory".into()),
                search_mode: SearchMode::FullText,
                limit: 10,
                filter: None,
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await
        .unwrap();
    }
    let elapsed = start.elapsed();
    let per_op = elapsed / n;

    eprintln!(
        "recall_fulltext (n=1000): {n} queries in {elapsed:?} ({per_op:?}/query)"
    );

    // Deferred access stats: was ~20ms, now ~6ms
    assert!(
        per_op.as_millis() < 20,
        "fulltext recall too slow: {per_op:?}/query (limit: 20ms)"
    );

    mast.close().unwrap();
}

#[tokio::test]
async fn recall_hybrid_latency() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(128);
    let mut mast = Mast::open(config).unwrap();
    seed(&mut mast, &embedder, "perf", 1000).await;

    let n = 50u32;
    let start = Instant::now();
    for _ in 0..n {
        mast.recall(
            RecallRequest {
                collection: "perf".into(),
                query: Some("performance memory".into()),
                query_embedding: None,
                text_query: Some("seeded memory topics".into()),
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
    }
    let elapsed = start.elapsed();
    let per_op = elapsed / n;

    eprintln!(
        "recall_hybrid (n=1000): {n} queries in {elapsed:?} ({per_op:?}/query)"
    );

    // Deferred access stats: was ~30ms, now ~11ms
    assert!(
        per_op.as_millis() < 30,
        "hybrid recall too slow: {per_op:?}/query (limit: 30ms)"
    );

    mast.close().unwrap();
}

#[tokio::test]
async fn recall_filtered_latency() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(128);
    let mut mast = Mast::open(config).unwrap();
    seed(&mut mast, &embedder, "perf", 1000).await;

    let n = 50u32;
    let start = Instant::now();
    for _ in 0..n {
        mast.recall(
            RecallRequest {
                collection: "perf".into(),
                query: Some("performance topics".into()),
                query_embedding: None,
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 10,
                filter: Some(MetadataFilter::Eq("group".into(), "2".into())),
                tier_weights: None,
                time_decay: None,
            },
            &embedder,
        )
        .await
        .unwrap();
    }
    let elapsed = start.elapsed();
    let per_op = elapsed / n;

    eprintln!(
        "recall_filtered (n=1000): {n} queries in {elapsed:?} ({per_op:?}/query)"
    );

    // Deferred access stats: now ~5ms
    assert!(
        per_op.as_millis() < 15,
        "filtered recall too slow: {per_op:?}/query (limit: 15ms)"
    );

    mast.close().unwrap();
}

// --- Scale test: store + recall at higher dimensionality ---

#[tokio::test]
async fn store_high_dimensional_throughput() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(1536); // OpenAI-size embeddings
    let mut mast = Mast::open(config).unwrap();

    let n = 50;
    let start = Instant::now();
    for i in 0..n {
        mast.store(
            StoreRequest {
                collection: "perf".into(),
                content: format!("high-dimensional doc {i}"),
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
    let elapsed = start.elapsed();
    let per_op = elapsed / n;

    eprintln!(
        "store_1536d: {n} ops in {elapsed:?} ({per_op:?}/op, {:.0} ops/s)",
        n as f64 / elapsed.as_secs_f64()
    );

    // 1536d vectors are larger; allow more time but should still be fast
    assert!(
        per_op.as_millis() < 30,
        "1536d store too slow: {per_op:?}/op (limit: 30ms)"
    );

    mast.close().unwrap();
}

// --- Delete throughput ---

#[tokio::test]
async fn delete_throughput() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(128);
    let mut mast = Mast::open(config).unwrap();
    seed(&mut mast, &embedder, "perf", 200).await;

    let memories = mast.list("perf").unwrap();
    let n = memories.len() as u32;

    let start = Instant::now();
    for m in &memories {
        mast.delete("perf", &m.id).unwrap();
    }
    let elapsed = start.elapsed();
    let per_op = elapsed / n;

    eprintln!(
        "delete: {n} ops in {elapsed:?} ({per_op:?}/op, {:.0} ops/s)",
        n as f64 / elapsed.as_secs_f64()
    );

    // Single-txn optimization: was ~100ms, now ~10ms
    assert!(
        per_op.as_millis() < 30,
        "delete too slow: {per_op:?}/op (limit: 30ms)"
    );
}

// --- Snapshot throughput ---

#[tokio::test]
async fn snapshot_throughput() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(128);
    let mut mast = Mast::open(config).unwrap();
    seed(&mut mast, &embedder, "perf", 1000).await;

    let start = Instant::now();
    let mut buf = Vec::new();
    mast.snapshot("perf", &mut buf).unwrap();
    let elapsed = start.elapsed();

    eprintln!(
        "snapshot 1000: {elapsed:?} ({:.1} KB)",
        buf.len() as f64 / 1024.0
    );

    assert!(
        elapsed.as_secs() < 2,
        "snapshot too slow: {elapsed:?} (limit: 2s)"
    );

    mast.close().unwrap();
}

// --- Restore throughput ---

#[tokio::test]
async fn restore_throughput() {
    let (_dir, config) = temp_config();
    let embedder = MockEmbedder::new(128);
    let mut mast = Mast::open(config).unwrap();
    seed(&mut mast, &embedder, "perf", 200).await;

    let mut buf = Vec::new();
    mast.snapshot("perf", &mut buf).unwrap();

    let dir2 = tempfile::tempdir().unwrap();
    let config2 = MastConfig::default().with_db_path(dir2.path().join("restore.mast"));
    let mut mast2 = Mast::open(config2).unwrap();

    let start = Instant::now();
    let mut cursor = std::io::Cursor::new(&buf);
    mast2.restore(&mut cursor, false).unwrap();
    let elapsed = start.elapsed();

    eprintln!("restore 200: {elapsed:?}");

    // Chunked batch txns: was ~22s, now ~350ms
    assert!(
        elapsed.as_secs() < 3,
        "restore too slow: {elapsed:?} (limit: 3s)"
    );

    // Verify data integrity
    let count = mast2.list("perf").unwrap().len();
    assert_eq!(count, 200);

    mast.close().unwrap();
    mast2.close().unwrap();
}
