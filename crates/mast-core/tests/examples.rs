//! Real-world usage examples that double as integration tests.
//!
//! Each test simulates a concrete use case end-to-end, exercising multiple
//! MAST features in the way an application would actually compose them.

use mast_core::compact::MockCompactor;
use mast_core::config::MastConfig;
use mast_core::embed::MockEmbedder;
use mast_core::types::*;
use mast_core::Mast;
use std::collections::HashMap;
use std::io::Cursor;

fn temp_mast() -> (tempfile::TempDir, Mast) {
    let dir = tempfile::tempdir().unwrap();
    let config = MastConfig::default().with_db_path(dir.path().join("test.mast"));
    let mast = Mast::open(config).unwrap();
    (dir, mast)
}

// ---------------------------------------------------------------------------
// 1. AI Chat Memory
//
// An AI assistant stores conversation turns, recalls relevant context for new
// messages, ages out old turns with tier demotion, and periodically compacts
// stale messages into summaries.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn example_ai_chat_memory() {
    let (_dir, mut mast) = temp_mast();
    let embedder = MockEmbedder::new(64);
    let compactor = MockCompactor;

    // -- Store a conversation --
    let conversation = [
        ("user", "I'm learning Rust and struggling with lifetimes"),
        ("assistant", "Lifetimes ensure references stay valid. Let me explain with an example."),
        ("user", "That makes sense. What about the borrow checker?"),
        ("assistant", "The borrow checker enforces ownership rules at compile time."),
        ("user", "Can you explain smart pointers like Box and Rc?"),
        ("assistant", "Box allocates on the heap. Rc enables shared ownership with reference counting."),
        ("user", "How do I use async/await in Rust?"),
        ("assistant", "Async functions return Futures. You need a runtime like tokio to execute them."),
        ("user", "What about error handling with Result?"),
        ("assistant", "Result<T, E> forces you to handle errors. Use the ? operator for propagation."),
    ];

    let mut memory_ids = Vec::new();
    for (i, (role, content)) in conversation.iter().enumerate() {
        let mem = mast
            .store(
                StoreRequest {
                    collection: "chat".into(),
                    content: content.to_string(),
                    embedding: None,
                    metadata: HashMap::from([
                        ("role".into(), role.to_string()),
                        ("turn".into(), i.to_string()),
                    ]),
                    tier: Tier::Active,
                    ttl: None,
                    created_at: None,
                },
                &embedder,
            )
            .await
            .unwrap();
        memory_ids.push(mem.id);
    }

    // -- Recall context for a new user message --
    // User now asks about lifetimes again — the early turns should surface.
    let results = mast
        .recall(
            RecallRequest {
                collection: "chat".into(),
                query: Some("explain Rust lifetimes and borrowing".into()),
                query_embedding: None,
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 3,
                filter: None,
                tier_weights: None,
                time_decay: None,
                reference_time: None,
                time_anchor: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 3);
    // All returned memories should be from the chat
    for r in &results {
        assert_eq!(r.memory.collection, "chat");
    }

    // -- Filter to only assistant responses --
    let assistant_results = mast
        .recall(
            RecallRequest {
                collection: "chat".into(),
                query: Some("explain Rust lifetimes".into()),
                query_embedding: None,
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 5,
                filter: Some(MetadataFilter::Eq("role".into(), "assistant".into())),
                tier_weights: None,
                time_decay: None,
                reference_time: None,
                time_anchor: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    for r in &assistant_results {
        assert_eq!(
            r.memory.metadata.get("role").unwrap(),
            "assistant",
            "filter should only return assistant messages"
        );
    }

    // -- Compact the first 4 messages into a summary --
    let early_ids: Vec<_> = memory_ids[..4].to_vec();
    let compact_result = mast
        .compact(
            CompactRequest {
                collection: "chat".into(),
                source_tier: Tier::Active,
                target_tier: Tier::Archive,
                strategy: CompactionStrategy::Manual { ids: early_ids.clone() },
                max_batch_size: 10,
                dry_run: false,
            },
            &compactor,
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(compact_result.source_count, 4);
    assert_eq!(compact_result.compacted_count, 4);
    assert_eq!(compact_result.new_memory_ids.len(), 1, "should produce one summary");

    // The summary's parent_ids link back to the originals
    let summary = mast
        .get("chat", &compact_result.new_memory_ids[0])
        .unwrap()
        .unwrap();
    assert_eq!(summary.parent_ids.len(), 4);
    assert_eq!(summary.tier, Tier::Archive);

    // -- Recall again: the summary participates, compacted originals stay at half weight --
    let post_compact = mast
        .recall(
            RecallRequest {
                collection: "chat".into(),
                query: Some("lifetimes and borrow checker".into()),
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
        )
        .await
        .unwrap();

    // 10 originals + 1 summary match; the limit keeps 10. A compacted original
    // scores at most similarity (1.0) * active tier (0.9) * compacted_source_weight (0.5).
    assert_eq!(post_compact.len(), 10);
    for r in post_compact.iter().filter(|r| early_ids.contains(&r.memory.id)) {
        assert!(r.memory.compacted_at.is_some());
        assert!(r.score <= 0.45 + 1e-6, "compacted original scored {}", r.score);
    }

    // -- Tier weights modify scores --
    // Recall with default weights, then with archive=0.0 to suppress it entirely
    let with_archive = mast
        .recall(
            RecallRequest {
                collection: "chat".into(),
                query: Some("lifetimes and borrow checker".into()),
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
        )
        .await
        .unwrap();

    let has_archive = with_archive.iter().any(|r| r.memory.tier == Tier::Archive);
    assert!(has_archive, "archived summary should appear with default weights");

    let without_archive = mast
        .recall(
            RecallRequest {
                collection: "chat".into(),
                query: Some("lifetimes and borrow checker".into()),
                query_embedding: None,
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 10,
                filter: None,
                tier_weights: Some(TierWeights {
                    core: 1.0,
                    active: 1.0,
                    background: 1.0,
                    archive: 0.0, // zero out archived memories
                }),
                time_decay: None,
                reference_time: None,
                time_anchor: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    // With archive weight=0.0, the summary gets score=0 and should be ranked last
    // (or not appear if all other scores are positive). Either way, verify the
    // tier weight was actually applied.
    let archive_result = without_archive.iter().find(|r| r.memory.tier == Tier::Archive);
    if let Some(r) = archive_result {
        assert_eq!(r.score, 0.0, "archive weight=0 should zero out the score");
    }
}

// ---------------------------------------------------------------------------
// 2. Knowledge Graph — Concept Mapping
//
// Build a knowledge graph of programming concepts, then navigate it via
// traversal with depth limits, relation filters, and weight thresholds.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn example_knowledge_graph() {
    let (_dir, mut mast) = temp_mast();
    let embedder = MockEmbedder::new(64);

    // -- Store concept nodes --
    let concepts = [
        ("Rust", "A systems programming language focused on safety and performance"),
        ("ownership", "Rust's core memory management model"),
        ("borrowing", "Temporary access to data without taking ownership"),
        ("lifetimes", "Annotations that tell the compiler how long references are valid"),
        ("async", "Asynchronous programming model using Futures"),
        ("tokio", "The most popular async runtime for Rust"),
        ("traits", "Rust's mechanism for shared behavior, similar to interfaces"),
        ("generics", "Parameterized types for writing flexible, reusable code"),
    ];

    let mut ids: HashMap<&str, ulid::Ulid> = HashMap::new();
    for (name, description) in &concepts {
        let mem = mast
            .store(
                StoreRequest {
                    collection: "concepts".into(),
                    content: description.to_string(),
                    embedding: None,
                    metadata: HashMap::from([("concept".into(), name.to_string())]),
                    tier: Tier::Core,
                    ttl: None,
                    created_at: None,
                },
                &embedder,
            )
            .await
            .unwrap();
        ids.insert(name, mem.id);
    }

    // -- Build edges between concepts --
    let edges = [
        ("Rust", "ownership", "has_concept", 1.0),
        ("Rust", "borrowing", "has_concept", 0.95),
        ("Rust", "lifetimes", "has_concept", 0.9),
        ("Rust", "async", "has_concept", 0.85),
        ("Rust", "traits", "has_concept", 0.9),
        ("Rust", "generics", "has_concept", 0.85),
        ("ownership", "borrowing", "enables", 0.95),
        ("borrowing", "lifetimes", "requires", 0.9),
        ("async", "tokio", "implemented_by", 0.8),
        ("traits", "generics", "used_with", 0.85),
    ];

    for (source, target, relation, weight) in &edges {
        mast.relate(RelateRequest {
            collection: "concepts".into(),
            source: source.to_string(),
            target: target.to_string(),
            relation: relation.to_string(),
            weight: *weight,
            memory_ids: vec![ids[source], ids[target]],
            valid_from: None,
            valid_to: None,
        })
        .unwrap();
    }

    // -- Traverse depth=1 from Rust: direct children only --
    let depth1 = mast
        .traverse(TraverseRequest {
            collection: "concepts".into(),
            start: "Rust".into(),
            max_depth: 1,
            relation_filter: None,
            min_weight: None,
            as_of: None,
        })
        .unwrap();

    let depth1_targets: Vec<&str> = depth1.iter().map(|e| e.target.as_str()).collect();
    assert!(depth1_targets.contains(&"ownership"));
    assert!(depth1_targets.contains(&"async"));
    assert!(depth1_targets.contains(&"traits"));
    // tokio is 2 hops away (Rust -> async -> tokio), should NOT appear
    assert!(!depth1_targets.contains(&"tokio"));

    // -- Traverse depth=2: reach tokio and lifetimes via intermediate nodes --
    let depth2 = mast
        .traverse(TraverseRequest {
            collection: "concepts".into(),
            start: "Rust".into(),
            max_depth: 2,
            relation_filter: None,
            min_weight: None,
            as_of: None,
        })
        .unwrap();

    let depth2_targets: Vec<&str> = depth2.iter().map(|e| e.target.as_str()).collect();
    assert!(depth2_targets.contains(&"tokio"), "tokio should be reachable at depth 2");
    assert!(depth2_targets.contains(&"lifetimes"), "lifetimes reachable via borrowing");

    // -- Filter by relation: only "has_concept" edges --
    let has_concept_only = mast
        .traverse(TraverseRequest {
            collection: "concepts".into(),
            start: "Rust".into(),
            max_depth: 3,
            relation_filter: Some("has_concept".into()),
            min_weight: None,
            as_of: None,
        })
        .unwrap();

    for edge in &has_concept_only {
        assert_eq!(edge.relation, "has_concept");
    }
    // "tokio" is only reachable via "implemented_by", not "has_concept"
    let hc_targets: Vec<&str> = has_concept_only.iter().map(|e| e.target.as_str()).collect();
    assert!(!hc_targets.contains(&"tokio"));

    // -- Filter by min_weight: exclude weaker connections --
    let strong_only = mast
        .traverse(TraverseRequest {
            collection: "concepts".into(),
            start: "Rust".into(),
            max_depth: 3,
            relation_filter: None,
            min_weight: Some(0.9),
            as_of: None,
        })
        .unwrap();

    for edge in &strong_only {
        assert!(
            edge.weight >= 0.9,
            "edge {}->{} weight {} below threshold",
            edge.source,
            edge.target,
            edge.weight
        );
    }

    // -- Subgraph: full picture --
    let graph = mast.subgraph("concepts").unwrap();
    assert_eq!(graph.edges.len(), edges.len());

    // -- Remove an edge and verify traversal changes --
    mast.unrelate("concepts", "async", "tokio", "implemented_by")
        .unwrap();

    let after_unrelate = mast
        .traverse(TraverseRequest {
            collection: "concepts".into(),
            start: "Rust".into(),
            max_depth: 3,
            relation_filter: None,
            min_weight: None,
            as_of: None,
        })
        .unwrap();

    let reachable: Vec<&str> = after_unrelate.iter().map(|e| e.target.as_str()).collect();
    assert!(
        !reachable.contains(&"tokio"),
        "tokio should be unreachable after removing async->tokio edge"
    );

    // -- Upsert: update the weight of an existing edge --
    mast.relate(RelateRequest {
        collection: "concepts".into(),
        source: "Rust".into(),
        target: "ownership".into(),
        relation: "has_concept".into(),
        weight: 0.5, // downgrade
        memory_ids: vec![],
        valid_from: None,
        valid_to: None,
    })
    .unwrap();

    // Now filter at 0.9 should exclude ownership
    let after_downgrade = mast
        .traverse(TraverseRequest {
            collection: "concepts".into(),
            start: "Rust".into(),
            max_depth: 1,
            relation_filter: None,
            min_weight: Some(0.9),
            as_of: None,
        })
        .unwrap();

    let targets: Vec<&str> = after_downgrade.iter().map(|e| e.target.as_str()).collect();
    assert!(
        !targets.contains(&"ownership"),
        "ownership should be excluded after weight downgrade to 0.5"
    );
}

// ---------------------------------------------------------------------------
// 3. RAG Knowledge Base
//
// A documentation site ingests pages into themed collections, then answers
// queries using vector, full-text, and hybrid search. Metadata filters
// narrow results to specific versions or categories.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn example_rag_knowledge_base() {
    let (_dir, mut mast) = temp_mast();
    let embedder = MockEmbedder::new(64);

    // -- Ingest documentation --
    let docs = [
        ("api", "v2", "Authentication uses Bearer tokens in the Authorization header"),
        ("api", "v2", "Rate limiting applies at 100 requests per minute per API key"),
        ("api", "v1", "Authentication uses basic auth with username and password"),
        ("tutorial", "v2", "Getting started: install the SDK with npm install mast-sdk"),
        ("tutorial", "v2", "Building your first app: create a new project and configure your API key"),
        ("changelog", "v2", "v2.0 release: new authentication system, improved rate limiting"),
        ("changelog", "v1", "v1.5 release: added batch endpoints and webhook support"),
    ];

    for (category, version, content) in &docs {
        mast.store(
            StoreRequest {
                collection: "docs".into(),
                content: content.to_string(),
                embedding: None,
                metadata: HashMap::from([
                    ("category".into(), category.to_string()),
                    ("version".into(), version.to_string()),
                ]),
                tier: Tier::Active,
                ttl: None,
                created_at: None,
            },
            &embedder,
        )
        .await
        .unwrap();
    }

    // -- Vector search: semantic similarity --
    let vector_results = mast
        .recall(
            RecallRequest {
                collection: "docs".into(),
                query: Some("how do I authenticate API requests".into()),
                query_embedding: None,
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 3,
                filter: None,
                tier_weights: None,
                time_decay: None,
                reference_time: None,
                time_anchor: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert!(!vector_results.is_empty());

    // -- Full-text search: keyword match --
    let ft_results = mast
        .recall(
            RecallRequest {
                collection: "docs".into(),
                query: None,
                query_embedding: None,
                text_query: Some("authentication Bearer tokens".into()),
                search_mode: SearchMode::FullText,
                limit: 3,
                filter: None,
                tier_weights: None,
                time_decay: None,
                reference_time: None,
                time_anchor: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert!(!ft_results.is_empty());
    assert!(
        ft_results[0].memory.content.contains("Bearer"),
        "top full-text result should contain the search terms"
    );

    // -- Hybrid search: combine both signals --
    let hybrid_results = mast
        .recall(
            RecallRequest {
                collection: "docs".into(),
                query: Some("authentication".into()),
                query_embedding: None,
                text_query: Some("authentication Bearer tokens".into()),
                search_mode: SearchMode::Hybrid {
                    vector_weight: 0.5,
                    text_weight: 0.5,
                },
                limit: 3,
                filter: None,
                tier_weights: None,
                time_decay: None,
                reference_time: None,
                time_anchor: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert!(!hybrid_results.is_empty());

    // -- Metadata filter: only v2 docs --
    let v2_only = mast
        .recall(
            RecallRequest {
                collection: "docs".into(),
                query: Some("authentication".into()),
                query_embedding: None,
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 10,
                filter: Some(MetadataFilter::Eq("version".into(), "v2".into())),
                tier_weights: None,
                time_decay: None,
                reference_time: None,
                time_anchor: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    for r in &v2_only {
        assert_eq!(
            r.memory.metadata.get("version").unwrap(),
            "v2",
            "all results should be v2"
        );
    }

    // -- Combined filter: v2 AND api category --
    let v2_api = mast
        .recall(
            RecallRequest {
                collection: "docs".into(),
                query: Some("authentication".into()),
                query_embedding: None,
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 10,
                filter: Some(MetadataFilter::And(vec![
                    MetadataFilter::Eq("version".into(), "v2".into()),
                    MetadataFilter::Eq("category".into(), "api".into()),
                ])),
                tier_weights: None,
                time_decay: None,
                reference_time: None,
                time_anchor: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    for r in &v2_api {
        assert_eq!(r.memory.metadata.get("version").unwrap(), "v2");
        assert_eq!(r.memory.metadata.get("category").unwrap(), "api");
    }

    // -- Full-text search with metadata filter --
    let ft_filtered = mast
        .recall(
            RecallRequest {
                collection: "docs".into(),
                query: None,
                query_embedding: None,
                text_query: Some("release".into()),
                search_mode: SearchMode::FullText,
                limit: 10,
                filter: Some(MetadataFilter::Eq("category".into(), "changelog".into())),
                tier_weights: None,
                time_decay: None,
                reference_time: None,
                time_anchor: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    for r in &ft_filtered {
        assert_eq!(r.memory.metadata.get("category").unwrap(), "changelog");
    }
}

// ---------------------------------------------------------------------------
// 4. Multi-Tenant SaaS — Collection Isolation
//
// Each tenant gets a collection. Data from one tenant never leaks into
// another's queries, snapshots, or restores.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn example_multi_tenant_isolation() {
    let (_dir, mut mast) = temp_mast();
    let embedder = MockEmbedder::new(64);

    // -- Tenant A: project management data --
    for content in [
        "Sprint planning meeting every Monday at 10am",
        "Use Jira for tracking tickets and story points",
        "Quarterly OKR review with the leadership team",
    ] {
        mast.store(
            StoreRequest {
                collection: "tenant_a".into(),
                content: content.into(),
                embedding: None,
                metadata: HashMap::from([("tenant".into(), "a".into())]),
                tier: Tier::Active,
                ttl: None,
                created_at: None,
            },
            &embedder,
        )
        .await
        .unwrap();
    }

    // -- Tenant B: engineering docs --
    for content in [
        "Deploy to production using GitHub Actions CI/CD pipeline",
        "Database migrations run automatically on deploy",
        "Use feature flags for gradual rollouts",
    ] {
        mast.store(
            StoreRequest {
                collection: "tenant_b".into(),
                content: content.into(),
                embedding: None,
                metadata: HashMap::from([("tenant".into(), "b".into())]),
                tier: Tier::Active,
                ttl: None,
                created_at: None,
            },
            &embedder,
        )
        .await
        .unwrap();
    }

    // -- Tenant C: customer support KB --
    for content in [
        "Reset your password from the account settings page",
        "Contact support at help@example.com for billing issues",
    ] {
        mast.store(
            StoreRequest {
                collection: "tenant_c".into(),
                content: content.into(),
                embedding: None,
                metadata: HashMap::from([("tenant".into(), "c".into())]),
                tier: Tier::Active,
                ttl: None,
                created_at: None,
            },
            &embedder,
        )
        .await
        .unwrap();
    }

    // -- Recall from tenant_a returns zero tenant_b/c data --
    let a_results = mast
        .recall(
            RecallRequest {
                collection: "tenant_a".into(),
                query: Some("deploy CI/CD pipeline".into()), // this is tenant_b content
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
        )
        .await
        .unwrap();

    for r in &a_results {
        assert_eq!(
            r.memory.collection, "tenant_a",
            "tenant_a recall should never return data from another tenant"
        );
    }

    // -- Info shows correct per-tenant counts --
    let info = mast.info().unwrap();
    let count_for = |name: &str| -> u64 {
        info.iter().find(|i| i.name == name).unwrap().memory_count
    };
    assert_eq!(count_for("tenant_a"), 3);
    assert_eq!(count_for("tenant_b"), 3);
    assert_eq!(count_for("tenant_c"), 2);

    // -- Snapshot tenant_a: contains no tenant_b data --
    let mut snap_buf = Vec::new();
    let snap = mast.snapshot("tenant_a", &mut snap_buf).unwrap();
    assert_eq!(snap.memory_count, 3);

    let snap_text = String::from_utf8_lossy(&snap_buf);
    assert!(!snap_text.contains("tenant_b"), "snapshot should not contain other tenants' data");
    assert!(!snap_text.contains("CI/CD"), "snapshot should not contain tenant_b content");

    // -- Restore tenant_a into a fresh DB: only tenant_a exists --
    let (_dir2, mut mast2) = temp_mast();
    let mut cursor = Cursor::new(&snap_buf);
    let restore = mast2.restore(&mut cursor, false).unwrap();
    assert_eq!(restore.memory_count, 3);

    let info2 = mast2.info().unwrap();
    assert_eq!(info2.len(), 1, "fresh DB should only have tenant_a");
    assert_eq!(info2[0].name, "tenant_a");

    // -- Delete all of tenant_b: tenant_a and tenant_c unaffected --
    let b_memories = mast.list("tenant_b").unwrap();
    for mem in &b_memories {
        mast.delete("tenant_b", &mem.id).unwrap();
    }

    assert_eq!(mast.list("tenant_b").unwrap().len(), 0);
    assert_eq!(mast.list("tenant_a").unwrap().len(), 3, "tenant_a unaffected");
    assert_eq!(mast.list("tenant_c").unwrap().len(), 2, "tenant_c unaffected");
}

// ---------------------------------------------------------------------------
// 5. Personal Journal with TTL + Time Decay
//
// Daily entries with metadata, time-based ranking, TTL expiration for
// ephemeral notes, vacuum cleanup, and snapshot/restore round-trip.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn example_personal_journal() {
    let (_dir, mut mast) = temp_mast();
    let embedder = MockEmbedder::new(64);

    // -- Store journal entries: some permanent, some ephemeral --
    let entries = [
        ("Great day at the office, shipped the new feature", "happy", "office", None),
        ("Feeling stressed about the deadline", "stressed", "office", None),
        ("Beautiful hike in the mountains this weekend", "happy", "outdoors", None),
        ("Quick thought: try the new coffee shop tomorrow", "neutral", "home", Some(1i64)), // TTL=1s, will expire
        ("Reminder: buy groceries on the way home", "neutral", "home", Some(1i64)),          // TTL=1s, will expire
        ("Deep conversation with a friend about life goals", "reflective", "cafe", None),
    ];

    let mut permanent_ids = Vec::new();
    for (content, mood, location, ttl) in &entries {
        let mem = mast
            .store(
                StoreRequest {
                    collection: "journal".into(),
                    content: content.to_string(),
                    embedding: None,
                    metadata: HashMap::from([
                        ("mood".into(), mood.to_string()),
                        ("location".into(), location.to_string()),
                    ]),
                    tier: Tier::Active,
                    ttl: *ttl,
                    created_at: None,
                },
                &embedder,
            )
            .await
            .unwrap();
        if ttl.is_none() {
            permanent_ids.push(mem.id);
        }
    }

    assert_eq!(mast.list("journal").unwrap().len(), 6);

    // -- Filter by mood --
    let happy = mast
        .recall(
            RecallRequest {
                collection: "journal".into(),
                query: Some("good memories".into()),
                query_embedding: None,
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 10,
                filter: Some(MetadataFilter::Eq("mood".into(), "happy".into())),
                tier_weights: None,
                time_decay: None,
                reference_time: None,
                time_anchor: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    for r in &happy {
        assert_eq!(r.memory.metadata.get("mood").unwrap(), "happy");
    }
    assert_eq!(happy.len(), 2);

    // -- Filter by location with OR --
    let out_of_office = mast
        .recall(
            RecallRequest {
                collection: "journal".into(),
                query: Some("weekend activities".into()),
                query_embedding: None,
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 10,
                filter: Some(MetadataFilter::Or(vec![
                    MetadataFilter::Eq("location".into(), "outdoors".into()),
                    MetadataFilter::Eq("location".into(), "cafe".into()),
                ])),
                tier_weights: None,
                time_decay: None,
                reference_time: None,
                time_anchor: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    for r in &out_of_office {
        let loc = r.memory.metadata.get("location").unwrap();
        assert!(
            loc == "outdoors" || loc == "cafe",
            "unexpected location: {loc}"
        );
    }

    // -- Wait for TTL to expire, then vacuum --
    // The ephemeral entries have TTL=1s. Sleep briefly, then vacuum.
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let vacuum_result = mast.vacuum("journal").unwrap();
    assert_eq!(vacuum_result.expired_count, 2, "two ephemeral entries should expire");

    let remaining = mast.list("journal").unwrap();
    assert_eq!(remaining.len(), 4, "only permanent entries remain");

    // Verify all remaining are the permanent ones
    for mem in &remaining {
        assert!(mem.ttl.is_none(), "remaining entries should have no TTL");
        assert!(permanent_ids.contains(&mem.id));
    }

    // -- Full-text search on remaining entries --
    let ft = mast
        .recall(
            RecallRequest {
                collection: "journal".into(),
                query: None,
                query_embedding: None,
                text_query: Some("office shipped feature".into()),
                search_mode: SearchMode::FullText,
                limit: 5,
                filter: None,
                tier_weights: None,
                time_decay: None,
                reference_time: None,
                time_anchor: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert!(!ft.is_empty());

    // -- Snapshot and restore: verify identical state --
    let mut snap_buf = Vec::new();
    let snap = mast.snapshot("journal", &mut snap_buf).unwrap();
    assert_eq!(snap.memory_count, 4);

    let (_dir2, mut mast2) = temp_mast();
    let mut cursor = Cursor::new(&snap_buf);
    let restore = mast2.restore(&mut cursor, false).unwrap();
    assert_eq!(restore.memory_count, 4);
    assert_eq!(restore.skipped_count, 0);

    // Recall on restored DB should return same results
    let restored_ft = mast2
        .recall(
            RecallRequest {
                collection: "journal".into(),
                query: None,
                query_embedding: None,
                text_query: Some("office shipped feature".into()),
                search_mode: SearchMode::FullText,
                limit: 5,
                filter: None,
                tier_weights: None,
                time_decay: None,
                reference_time: None,
                time_anchor: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert_eq!(restored_ft.len(), ft.len());
}

// ---------------------------------------------------------------------------
// 6. Snapshot Backup & Migration Pipeline
//
// Build a collection, snapshot it, restore into a fresh DB, mutate the
// original, then use merge to sync the delta.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn example_snapshot_backup_pipeline() {
    let (_dir, mut mast) = temp_mast();
    let embedder = MockEmbedder::new(64);

    // -- Build initial collection with memories + edges --
    let mut all_ids = Vec::new();
    for i in 0..20 {
        let mem = mast
            .store(
                StoreRequest {
                    collection: "project".into(),
                    content: format!("document number {i} about topic {}", i % 5),
                    embedding: None,
                    metadata: HashMap::from([("idx".into(), i.to_string())]),
                    tier: Tier::Active,
                    ttl: None,
                    created_at: None,
                },
                &embedder,
            )
            .await
            .unwrap();
        all_ids.push(mem.id);
    }

    // Add some edges
    for pair in [("intro", "basics"), ("basics", "advanced"), ("advanced", "expert")] {
        mast.relate(RelateRequest {
            collection: "project".into(),
            source: pair.0.into(),
            target: pair.1.into(),
            relation: "prerequisite".into(),
            weight: 1.0,
            memory_ids: vec![],
            valid_from: None,
            valid_to: None,
        })
        .unwrap();
    }

    // -- Snapshot --
    let mut snap_buf = Vec::new();
    let snap = mast.snapshot("project", &mut snap_buf).unwrap();
    assert_eq!(snap.memory_count, 20);
    assert_eq!(snap.edge_count, 3);

    // -- Restore to fresh DB --
    let (_dir2, mut backup) = temp_mast();
    let mut cursor = Cursor::new(&snap_buf);
    let restore = backup.restore(&mut cursor, false).unwrap();
    assert_eq!(restore.memory_count, 20);
    assert_eq!(restore.edge_count, 3);

    // Recall produces results on backup
    let backup_results = backup
        .recall(
            RecallRequest {
                collection: "project".into(),
                query: Some("document about topic".into()),
                query_embedding: None,
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 5,
                filter: None,
                tier_weights: None,
                time_decay: None,
                reference_time: None,
                time_anchor: None,
            },
            &embedder,
        )
        .await
        .unwrap();
    assert_eq!(backup_results.len(), 5);

    // -- Merge same snapshot again: everything skipped --
    let mut cursor2 = Cursor::new(&snap_buf);
    let merge_dup = backup.restore(&mut cursor2, true).unwrap();
    assert_eq!(merge_dup.memory_count, 0, "all memories already exist");
    assert_eq!(merge_dup.edge_count, 0, "all edges already exist");
    assert_eq!(merge_dup.skipped_count, 23, "20 memories + 3 edges skipped");

    // -- Mutate original: add 5 new memories, delete 3 --
    for i in 20..25 {
        mast.store(
            StoreRequest {
                collection: "project".into(),
                content: format!("new document {i}"),
                embedding: None,
                metadata: HashMap::from([("idx".into(), i.to_string())]),
                tier: Tier::Active,
                ttl: None,
                created_at: None,
            },
            &embedder,
        )
        .await
        .unwrap();
    }
    for id in &all_ids[..3] {
        mast.delete("project", id).unwrap();
    }

    assert_eq!(mast.list("project").unwrap().len(), 22); // 20 - 3 + 5

    // -- Snapshot the mutated state --
    let mut snap_buf2 = Vec::new();
    let snap2 = mast.snapshot("project", &mut snap_buf2).unwrap();
    assert_eq!(snap2.memory_count, 22);

    // -- Merge into backup: only the 5 new ones should be added --
    // (The 3 deleted memories still exist in backup — merge doesn't delete.)
    let mut cursor3 = Cursor::new(&snap_buf2);
    let merge_delta = backup.restore(&mut cursor3, true).unwrap();
    assert_eq!(merge_delta.memory_count, 5, "only 5 new memories added");
    // The 17 that overlap with backup's 20 + 3 edges = 20 skipped
    assert_eq!(merge_delta.skipped_count, 20);

    // Backup now has 20 (original) + 5 (new) = 25
    assert_eq!(backup.list("project").unwrap().len(), 25);
}

// ---------------------------------------------------------------------------
// 7. Hybrid Search Comparison
//
// Store content where vector similarity and keyword relevance diverge,
// then show how hybrid search combines both signals.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn example_hybrid_search_comparison() {
    let (_dir, mut mast) = temp_mast();
    let embedder = MockEmbedder::new(64);

    // These documents have varying overlap between semantic meaning and keywords
    let docs = [
        "The quick brown fox jumps over the lazy dog",
        "A fast auburn canine leaps above a sleepy hound",  // semantically similar, different keywords
        "Machine learning algorithms for natural language processing",
        "NLP models use neural networks for text classification",
        "The weather forecast predicts rain for next Tuesday",
        "fox terrier is a popular dog breed known for hunting",  // keyword overlap with doc 0
    ];

    for content in &docs {
        mast.store(
            StoreRequest {
                collection: "search_test".into(),
                content: content.to_string(),
                embedding: None,
                metadata: HashMap::new(),
                tier: Tier::Active,
                ttl: None,
                created_at: None,
            },
            &embedder,
        )
        .await
        .unwrap();
    }

    // -- Full-text: "fox dog" matches by keywords --
    let ft_results = mast
        .recall(
            RecallRequest {
                collection: "search_test".into(),
                query: None,
                query_embedding: None,
                text_query: Some("fox dog".into()),
                search_mode: SearchMode::FullText,
                limit: 3,
                filter: None,
                tier_weights: None,
                time_decay: None,
                reference_time: None,
                time_anchor: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert!(!ft_results.is_empty());
    // Full-text should find docs with literal "fox" and "dog"
    let ft_contents: Vec<&str> = ft_results.iter().map(|r| r.memory.content.as_str()).collect();
    assert!(
        ft_contents.iter().any(|c| c.contains("fox") || c.contains("dog")),
        "full-text should match literal keywords"
    );

    // -- Vector search: same query through embedder --
    let vec_results = mast
        .recall(
            RecallRequest {
                collection: "search_test".into(),
                query: Some("fox dog".into()),
                query_embedding: None,
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 3,
                filter: None,
                tier_weights: None,
                time_decay: None,
                reference_time: None,
                time_anchor: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert!(!vec_results.is_empty());

    // -- Hybrid: combines both signals --
    let hybrid_results = mast
        .recall(
            RecallRequest {
                collection: "search_test".into(),
                query: Some("fox dog".into()),
                query_embedding: None,
                text_query: Some("fox dog".into()),
                search_mode: SearchMode::Hybrid {
                    vector_weight: 0.5,
                    text_weight: 0.5,
                },
                limit: 3,
                filter: None,
                tier_weights: None,
                time_decay: None,
                reference_time: None,
                time_anchor: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    assert!(!hybrid_results.is_empty());

    // -- Verify that all three modes return results and scores are positive --
    for results in [&ft_results, &vec_results, &hybrid_results] {
        for r in results.iter() {
            assert!(r.score > 0.0, "scores should be positive");
        }
    }

    // -- Text-heavy hybrid vs vector-heavy hybrid --
    let text_heavy = mast
        .recall(
            RecallRequest {
                collection: "search_test".into(),
                query: Some("fox dog".into()),
                query_embedding: None,
                text_query: Some("fox dog".into()),
                search_mode: SearchMode::Hybrid {
                    vector_weight: 0.1,
                    text_weight: 0.9,
                },
                limit: 3,
                filter: None,
                tier_weights: None,
                time_decay: None,
                reference_time: None,
                time_anchor: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    let vector_heavy = mast
        .recall(
            RecallRequest {
                collection: "search_test".into(),
                query: Some("fox dog".into()),
                query_embedding: None,
                text_query: Some("fox dog".into()),
                search_mode: SearchMode::Hybrid {
                    vector_weight: 0.9,
                    text_weight: 0.1,
                },
                limit: 3,
                filter: None,
                tier_weights: None,
                time_decay: None,
                reference_time: None,
                time_anchor: None,
            },
            &embedder,
        )
        .await
        .unwrap();

    // Both should return results; rankings may differ
    assert!(!text_heavy.is_empty());
    assert!(!vector_heavy.is_empty());
}
