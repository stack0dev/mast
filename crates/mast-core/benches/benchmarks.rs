use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use mast_core::compact::MockCompactor;
use mast_core::config::MastConfig;
use mast_core::embed::MockEmbedder;
use mast_core::types::*;
use mast_core::Mast;
use std::collections::HashMap;

fn temp_mast(dims: usize) -> (tempfile::TempDir, Mast, MockEmbedder) {
    let dir = tempfile::tempdir().unwrap();
    let config = MastConfig::default().with_db_path(dir.path().join("bench.mast"));
    let mast = Mast::open(config).unwrap();
    let embedder = MockEmbedder::new(dims);
    (dir, mast, embedder)
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn seed_memories(
    mast: &mut Mast,
    embedder: &MockEmbedder,
    rt: &tokio::runtime::Runtime,
    collection: &str,
    count: usize,
) {
    let items: Vec<BatchStoreItem> = (0..count)
        .map(|i| BatchStoreItem {
            content: format!("benchmark memory content number {i} with some text for indexing"),
            embedding: None,
            metadata: HashMap::from([
                ("idx".into(), i.to_string()),
                ("group".into(), (i % 5).to_string()),
            ]),
            tier: Tier::Active,
            ttl: None,
        })
        .collect();

    for chunk in items.chunks(100) {
        let request = BatchStoreRequest {
            collection: collection.to_string(),
            items: chunk.to_vec(),
        };
        rt.block_on(mast.store_batch(request, embedder)).unwrap();
    }
}

fn bench_store_single(c: &mut Criterion) {
    let mut group = c.benchmark_group("store_single");
    for dims in [128, 768, 1536] {
        group.bench_with_input(BenchmarkId::from_parameter(dims), &dims, |b, &dims| {
            let rt = runtime();
            let (_dir, mut mast, embedder) = temp_mast(dims);
            let mut i = 0u64;
            b.iter(|| {
                let request = StoreRequest {
                    collection: "bench".into(),
                    content: format!("store single {i}"),
                    embedding: None,
                    metadata: HashMap::new(),
                    tier: Tier::Active,
                    ttl: None,
                };
                rt.block_on(mast.store(request, &embedder)).unwrap();
                i += 1;
            });
        });
    }
    group.finish();
}

fn bench_store_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("store_batch");
    for (batch_size, dims) in [(100, 128), (100, 768), (1000, 128)] {
        group.bench_with_input(
            BenchmarkId::new(format!("batch_{batch_size}"), dims),
            &(batch_size, dims),
            |b, &(batch_size, dims)| {
                let rt = runtime();
                let (_dir, mut mast, embedder) = temp_mast(dims);
                let mut round = 0u64;
                b.iter(|| {
                    let items: Vec<BatchStoreItem> = (0..batch_size)
                        .map(|i| BatchStoreItem {
                            content: format!("batch {round} item {i}"),
                            embedding: None,
                            metadata: HashMap::new(),
                            tier: Tier::Active,
                            ttl: None,
                        })
                        .collect();
                    let request = BatchStoreRequest {
                        collection: "bench".into(),
                        items,
                    };
                    rt.block_on(mast.store_batch(request, &embedder)).unwrap();
                    round += 1;
                });
            },
        );
    }
    group.finish();
}

fn bench_recall_vector(c: &mut Criterion) {
    let mut group = c.benchmark_group("recall_vector");
    for (count, dims) in [(100, 128), (1000, 128), (1000, 768)] {
        let rt = runtime();
        let (_dir, mut mast, embedder) = temp_mast(dims);
        seed_memories(&mut mast, &embedder, &rt, "bench", count);

        group.bench_with_input(
            BenchmarkId::new(format!("n{count}"), dims),
            &(),
            |b, _| {
                b.iter(|| {
                    let request = RecallRequest {
                        collection: "bench".into(),
                        query: Some("benchmark memory content".into()),
                        query_embedding: None,
                        text_query: None,
                        search_mode: SearchMode::Vector,
                        limit: 10,
                        filter: None,
                        tier_weights: None,
                        time_decay: None,
                    };
                    rt.block_on(mast.recall(request, &embedder)).unwrap();
                });
            },
        );
    }
    group.finish();
}

fn bench_recall_filtered(c: &mut Criterion) {
    let rt = runtime();
    let (_dir, mut mast, embedder) = temp_mast(128);
    seed_memories(&mut mast, &embedder, &rt, "bench", 1000);

    c.bench_function("recall_filtered_1000", |b| {
        b.iter(|| {
            let request = RecallRequest {
                collection: "bench".into(),
                query: Some("benchmark content".into()),
                query_embedding: None,
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 10,
                filter: Some(MetadataFilter::Eq("group".into(), "2".into())),
                tier_weights: None,
                time_decay: None,
            };
            rt.block_on(mast.recall(request, &embedder)).unwrap();
        });
    });
}

fn bench_recall_bm25(c: &mut Criterion) {
    let rt = runtime();
    let (_dir, mut mast, embedder) = temp_mast(128);
    seed_memories(&mut mast, &embedder, &rt, "bench", 1000);

    c.bench_function("recall_bm25_1000", |b| {
        b.iter(|| {
            let request = RecallRequest {
                collection: "bench".into(),
                query: None,
                query_embedding: None,
                text_query: Some("benchmark memory content indexing".into()),
                search_mode: SearchMode::FullText,
                limit: 10,
                filter: None,
                tier_weights: None,
                time_decay: None,
            };
            rt.block_on(mast.recall(request, &embedder)).unwrap();
        });
    });
}

fn bench_recall_hybrid(c: &mut Criterion) {
    let rt = runtime();
    let (_dir, mut mast, embedder) = temp_mast(128);
    seed_memories(&mut mast, &embedder, &rt, "bench", 1000);

    c.bench_function("recall_hybrid_1000", |b| {
        b.iter(|| {
            let request = RecallRequest {
                collection: "bench".into(),
                query: Some("benchmark content".into()),
                query_embedding: None,
                text_query: Some("benchmark memory content".into()),
                search_mode: SearchMode::Hybrid {
                    vector_weight: 0.5,
                    text_weight: 0.5,
                },
                limit: 10,
                filter: None,
                tier_weights: None,
                time_decay: None,
            };
            rt.block_on(mast.recall(request, &embedder)).unwrap();
        });
    });
}

fn bench_delete(c: &mut Criterion) {
    let rt = runtime();
    let (_dir, mut mast, embedder) = temp_mast(128);
    seed_memories(&mut mast, &embedder, &rt, "bench", 1000);

    let memories = mast.list("bench").unwrap();
    let mut idx = 0;

    c.bench_function("delete_single", |b| {
        b.iter(|| {
            if idx < memories.len() {
                mast.delete("bench", &memories[idx].id).unwrap();
                idx += 1;
            }
        });
    });
}

fn bench_vacuum(c: &mut Criterion) {
    let rt = runtime();
    let (_dir, mut mast, embedder) = temp_mast(128);

    // Create half with TTL=1 (already expired)
    for i in 0..500 {
        let request = StoreRequest {
            collection: "bench".into(),
            content: format!("expiring {i}"),
            embedding: None,
            metadata: HashMap::new(),
            tier: Tier::Active,
            ttl: Some(1),
        };
        rt.block_on(mast.store(request, &embedder)).unwrap();
    }
    for i in 0..500 {
        let request = StoreRequest {
            collection: "bench".into(),
            content: format!("permanent {i}"),
            embedding: None,
            metadata: HashMap::new(),
            tier: Tier::Active,
            ttl: None,
        };
        rt.block_on(mast.store(request, &embedder)).unwrap();
    }

    // Wait for TTL expiry
    std::thread::sleep(std::time::Duration::from_secs(2));

    c.bench_function("vacuum_expired_500", |b| {
        b.iter(|| {
            mast.vacuum("bench").unwrap();
        });
    });
}

fn bench_compact(c: &mut Criterion) {
    let rt = runtime();
    let compactor = MockCompactor;

    c.bench_function("compact_100", |b| {
        b.iter_custom(|iters| {
            let mut total = std::time::Duration::ZERO;
            for _ in 0..iters {
                let (_dir, mut mast, embedder) = temp_mast(128);
                seed_memories(&mut mast, &embedder, &rt, "bench", 100);

                let ids: Vec<ulid::Ulid> = mast.list("bench").unwrap().iter().map(|m| m.id).collect();
                let request = CompactRequest {
                    collection: "bench".into(),
                    source_tier: Tier::Active,
                    target_tier: Tier::Archive,
                    strategy: CompactionStrategy::Manual { ids },
                    max_batch_size: 100,
                    dry_run: false,
                };

                let start = std::time::Instant::now();
                rt.block_on(mast.compact(request, &compactor, &embedder))
                    .unwrap();
                total += start.elapsed();
            }
            total
        });
    });
}

fn bench_snapshot(c: &mut Criterion) {
    let rt = runtime();
    let (_dir, mut mast, embedder) = temp_mast(128);
    seed_memories(&mut mast, &embedder, &rt, "bench", 1000);

    // Add some edges
    for i in 0..50 {
        mast.relate(RelateRequest {
            collection: "bench".into(),
            source: format!("entity_{i}"),
            target: format!("entity_{}", i + 1),
            relation: "linked".into(),
            weight: 1.0,
            memory_ids: vec![],
        })
        .unwrap();
    }

    c.bench_function("snapshot_1000", |b| {
        b.iter(|| {
            let mut buf = Vec::new();
            mast.snapshot("bench", &mut buf).unwrap();
        });
    });
}

fn bench_restore(c: &mut Criterion) {
    let rt = runtime();
    let (_dir, mut mast, embedder) = temp_mast(128);
    seed_memories(&mut mast, &embedder, &rt, "bench", 1000);

    let mut buf = Vec::new();
    mast.snapshot("bench", &mut buf).unwrap();

    c.bench_function("restore_1000", |b| {
        b.iter_custom(|iters| {
            let mut total = std::time::Duration::ZERO;
            for _ in 0..iters {
                let dir = tempfile::tempdir().unwrap();
                let config = MastConfig::default().with_db_path(dir.path().join("restore.mast"));
                let mut mast2 = Mast::open(config).unwrap();

                let start = std::time::Instant::now();
                let mut cursor = std::io::Cursor::new(&buf);
                mast2.restore(&mut cursor, false).unwrap();
                total += start.elapsed();
            }
            total
        });
    });
}

fn bench_relate(c: &mut Criterion) {
    let (_dir, mast, _embedder) = temp_mast(128);
    let mut i = 0u64;

    c.bench_function("relate_edge", |b| {
        b.iter(|| {
            mast.relate(RelateRequest {
                collection: "bench".into(),
                source: format!("src_{i}"),
                target: format!("tgt_{i}"),
                relation: "linked".into(),
                weight: 1.0,
                memory_ids: vec![],
            })
            .unwrap();
            i += 1;
        });
    });
}

fn bench_traverse(c: &mut Criterion) {
    let (_dir, mast, _embedder) = temp_mast(128);

    // Build a graph: 100 entities, each linked to 3 others
    for i in 0..100 {
        for j in 1..=3 {
            let target = (i * 3 + j) % 100;
            mast.relate(RelateRequest {
                collection: "bench".into(),
                source: format!("entity_{i}"),
                target: format!("entity_{target}"),
                relation: "linked".into(),
                weight: 1.0,
                memory_ids: vec![],
            })
            .unwrap();
        }
    }

    c.bench_function("traverse_depth3", |b| {
        b.iter(|| {
            mast.traverse(TraverseRequest {
                collection: "bench".into(),
                start: "entity_0".into(),
                max_depth: 3,
                relation_filter: None,
                min_weight: None,
            })
            .unwrap();
        });
    });
}

criterion_group!(
    benches,
    bench_store_single,
    bench_store_batch,
    bench_recall_vector,
    bench_recall_filtered,
    bench_recall_bm25,
    bench_recall_hybrid,
    bench_delete,
    bench_vacuum,
    bench_compact,
    bench_snapshot,
    bench_restore,
    bench_relate,
    bench_traverse,
);
criterion_main!(benches);
