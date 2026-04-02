use clap::{Parser, Subcommand};
use mast_core::compact::MockCompactor;
use mast_core::config::MastConfig;
use mast_core::embed::MockEmbedder;
use mast_core::types::*;
use mast_core::Mast;
use mast_embed_local::LocalEmbedder;
use mast_embed_openai::OpenAIEmbedder;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "mast", about = "Memory storage engine")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Store a memory
    Store {
        /// Path to database file
        #[arg(long)]
        db: PathBuf,

        /// Collection name
        #[arg(long)]
        collection: String,

        /// Memory content
        #[arg(long)]
        content: String,

        /// Pre-computed embedding (comma-separated floats)
        #[arg(long)]
        embedding: Option<String>,

        /// Metadata key=value pairs
        #[arg(long, value_parser = parse_metadata)]
        metadata: Vec<(String, String)>,

        /// Memory tier
        #[arg(long, default_value = "active")]
        tier: String,

        /// TTL in seconds
        #[arg(long)]
        ttl: Option<i64>,

        /// Use local on-device embedder (no API key needed)
        #[arg(long)]
        local: bool,
    },

    /// Recall memories by similarity
    Recall {
        /// Path to database file
        #[arg(long)]
        db: PathBuf,

        /// Collection name
        #[arg(long)]
        collection: String,

        /// Natural language query (requires OPENAI_API_KEY or --local)
        #[arg(long)]
        query: Option<String>,

        /// Pre-computed query vector (comma-separated floats)
        #[arg(long)]
        query_vec: Option<String>,

        /// Full-text search query
        #[arg(long)]
        text_query: Option<String>,

        /// Search mode: vector, fulltext, hybrid
        #[arg(long, default_value = "vector")]
        search_mode: String,

        /// Maximum results
        #[arg(long, default_value = "10")]
        limit: usize,

        /// Metadata filter key=value pairs (AND semantics)
        #[arg(long, value_parser = parse_metadata)]
        filter: Vec<(String, String)>,

        /// Use local on-device embedder (no API key needed)
        #[arg(long)]
        local: bool,
    },

    /// Show database info
    Info {
        /// Path to database file
        #[arg(long)]
        db: PathBuf,
    },

    /// Delete a memory by ID
    Delete {
        /// Path to database file
        #[arg(long)]
        db: PathBuf,

        /// Collection name
        #[arg(long)]
        collection: String,

        /// Memory ID (ULID)
        #[arg(long)]
        id: String,
    },

    /// List all memories in a collection
    List {
        /// Path to database file
        #[arg(long)]
        db: PathBuf,

        /// Collection name
        #[arg(long)]
        collection: String,
    },

    /// Remove expired memories
    Vacuum {
        /// Path to database file
        #[arg(long)]
        db: PathBuf,

        /// Collection name
        #[arg(long)]
        collection: String,
    },

    /// Compact memories from one tier to another
    Compact {
        /// Path to database file
        #[arg(long)]
        db: PathBuf,

        /// Collection name
        #[arg(long)]
        collection: String,

        /// Source tier
        #[arg(long)]
        source_tier: String,

        /// Target tier
        #[arg(long)]
        target_tier: String,

        /// Compaction strategy: age, access_decay, hybrid
        #[arg(long, default_value = "age")]
        strategy: String,

        /// For age strategy: minimum age in seconds
        #[arg(long, default_value = "86400")]
        older_than: i64,

        /// Maximum batch size for compaction
        #[arg(long, default_value = "10")]
        max_batch_size: usize,

        /// Dry run: show candidates without compacting
        #[arg(long)]
        dry_run: bool,
    },

    /// Export a collection to a JSONL snapshot file
    Snapshot {
        /// Path to database file
        #[arg(long)]
        db: PathBuf,

        /// Collection name
        #[arg(long)]
        collection: String,

        /// Output file path
        #[arg(long)]
        output: PathBuf,
    },

    /// Restore a collection from a JSONL snapshot file
    Restore {
        /// Path to database file
        #[arg(long)]
        db: PathBuf,

        /// Input file path
        #[arg(long)]
        input: PathBuf,

        /// Merge into existing collection (skip duplicates)
        #[arg(long)]
        merge: bool,
    },

    /// Create or update a graph edge between entities
    Relate {
        /// Path to database file
        #[arg(long)]
        db: PathBuf,

        /// Collection name
        #[arg(long)]
        collection: String,

        /// Source entity
        #[arg(long)]
        source: String,

        /// Target entity
        #[arg(long)]
        target: String,

        /// Relation type
        #[arg(long)]
        relation: String,

        /// Edge weight
        #[arg(long, default_value = "1.0")]
        weight: f32,
    },

    /// Traverse the graph from a starting entity
    Traverse {
        /// Path to database file
        #[arg(long)]
        db: PathBuf,

        /// Collection name
        #[arg(long)]
        collection: String,

        /// Starting entity
        #[arg(long)]
        start: String,

        /// Maximum traversal depth
        #[arg(long, default_value = "3")]
        max_depth: usize,

        /// Filter by relation type
        #[arg(long)]
        relation: Option<String>,

        /// Minimum edge weight
        #[arg(long)]
        min_weight: Option<f32>,
    },
}

fn parse_metadata(s: &str) -> Result<(String, String), String> {
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid KEY=VALUE: no '=' found in '{s}'"))?;
    Ok((s[..pos].to_string(), s[pos + 1..].to_string()))
}

fn parse_embedding(s: &str) -> Result<Vec<f32>, String> {
    s.split(',')
        .map(|v| {
            v.trim()
                .parse::<f32>()
                .map_err(|e| format!("invalid float '{v}': {e}"))
        })
        .collect()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Store {
            db,
            collection,
            content,
            embedding,
            metadata,
            tier,
            ttl,
            local,
        } => {
            let config = MastConfig::default().with_db_path(&db);
            let mut mast = Mast::open(config)?;

            let parsed_embedding = embedding.map(|e| parse_embedding(&e)).transpose()?;
            let tier: Tier = tier.parse().map_err(|e: String| e)?;

            let metadata_map: HashMap<String, String> = metadata.into_iter().collect();

            let embedder: Box<dyn mast_core::embed::Embedder> = if parsed_embedding.is_some() {
                Box::new(MockEmbedder::new(0))
            } else if local {
                Box::new(LocalEmbedder::new()?)
            } else {
                let api_key = std::env::var("OPENAI_API_KEY").map_err(|_| {
                    "OPENAI_API_KEY env var required when --embedding and --local are not provided"
                })?;
                Box::new(OpenAIEmbedder::new(api_key))
            };

            let request = StoreRequest {
                collection,
                content,
                embedding: parsed_embedding,
                metadata: metadata_map,
                tier,
                ttl,
            };

            let memory = mast.store(request, embedder.as_ref()).await?;
            mast.close()?;

            println!("Stored memory: {}", memory.id);
        }

        Commands::Recall {
            db,
            collection,
            query,
            query_vec,
            text_query,
            search_mode,
            limit,
            filter,
            local,
        } => {
            let config = MastConfig::default().with_db_path(&db);
            let mut mast = Mast::open(config)?;

            let query_embedding = query_vec.map(|v| parse_embedding(&v)).transpose()?;
            let search_mode: SearchMode = search_mode.parse().map_err(|e: String| e)?;

            // Validate inputs based on search mode
            match &search_mode {
                SearchMode::Vector => {
                    if query.is_none() && query_embedding.is_none() {
                        return Err("either --query or --query-vec must be provided for vector search".into());
                    }
                }
                SearchMode::FullText => {
                    if text_query.is_none() && query.is_none() {
                        return Err("--text-query or --query must be provided for fulltext search".into());
                    }
                }
                SearchMode::Hybrid { .. } => {
                    if query.is_none() && query_embedding.is_none() {
                        return Err("--query or --query-vec must be provided for hybrid search".into());
                    }
                }
            }

            let needs_embedder = !matches!(search_mode, SearchMode::FullText);
            let embedder: Box<dyn mast_core::embed::Embedder> =
                if query_embedding.is_some() || !needs_embedder {
                    Box::new(MockEmbedder::new(0))
                } else if local {
                    Box::new(LocalEmbedder::new()?)
                } else {
                    let api_key = std::env::var("OPENAI_API_KEY").map_err(|_| {
                        "OPENAI_API_KEY env var required when --query-vec and --local are not provided"
                    })?;
                    Box::new(OpenAIEmbedder::new(api_key))
                };

            let metadata_filter = if filter.is_empty() {
                None
            } else {
                let filters: Vec<MetadataFilter> = filter
                    .into_iter()
                    .map(|(k, v)| MetadataFilter::Eq(k, v))
                    .collect();
                if filters.len() == 1 {
                    Some(filters.into_iter().next().unwrap())
                } else {
                    Some(MetadataFilter::And(filters))
                }
            };

            let request = RecallRequest {
                collection,
                query,
                query_embedding,
                text_query,
                search_mode,
                limit,
                filter: metadata_filter,
                tier_weights: None,
                time_decay: None,
            };

            let results = mast.recall(request, embedder.as_ref()).await?;

            if results.is_empty() {
                println!("No results found.");
            } else {
                for (i, result) in results.iter().enumerate() {
                    let preview = if result.memory.content.len() > 80 {
                        format!("{}...", &result.memory.content[..80])
                    } else {
                        result.memory.content.clone()
                    };
                    println!(
                        "{}. [score={:.4}] {} | {}",
                        i + 1,
                        result.score,
                        result.memory.id,
                        preview,
                    );
                    if !result.memory.metadata.is_empty() {
                        let meta: Vec<String> = result
                            .memory
                            .metadata
                            .iter()
                            .map(|(k, v)| format!("{k}={v}"))
                            .collect();
                        println!("   metadata: {}", meta.join(", "));
                    }
                }
            }
        }

        Commands::Info { db } => {
            let config = MastConfig::default().with_db_path(&db);
            let mast = Mast::open(config)?;
            let collections = mast.info()?;

            if collections.is_empty() {
                println!("No collections found.");
            } else {
                println!("{:<30} {:>10} {:>10}", "Collection", "Memories", "Dims");
                println!("{}", "-".repeat(52));
                for col in &collections {
                    println!(
                        "{:<30} {:>10} {:>10}",
                        col.name, col.memory_count, col.dimensions
                    );
                }
            }
        }

        Commands::Delete {
            db,
            collection,
            id,
        } => {
            let config = MastConfig::default().with_db_path(&db);
            let mut mast = Mast::open(config)?;

            let ulid: ulid::Ulid = id
                .parse()
                .map_err(|e| format!("invalid ULID '{id}': {e}"))?;

            mast.delete(&collection, &ulid)?;
            println!("Deleted memory: {id}");
        }

        Commands::List { db, collection } => {
            let config = MastConfig::default().with_db_path(&db);
            let mast = Mast::open(config)?;

            let memories = mast.list(&collection)?;

            if memories.is_empty() {
                println!("No memories found.");
            } else {
                for mem in &memories {
                    let preview = if mem.content.len() > 60 {
                        format!("{}...", &mem.content[..60])
                    } else {
                        mem.content.clone()
                    };
                    let compacted = if mem.compacted_at.is_some() {
                        " [compacted]"
                    } else {
                        ""
                    };
                    println!(
                        "{} | {} | {}{}",
                        mem.id, mem.tier, preview, compacted
                    );
                }
                println!("\nTotal: {}", memories.len());
            }
        }

        Commands::Vacuum { db, collection } => {
            let config = MastConfig::default().with_db_path(&db);
            let mut mast = Mast::open(config)?;

            let result = mast.vacuum(&collection)?;
            println!(
                "Vacuum complete: {} expired memories removed from '{}'",
                result.expired_count, result.collection
            );
        }

        Commands::Compact {
            db,
            collection,
            source_tier,
            target_tier,
            strategy,
            older_than,
            max_batch_size,
            dry_run,
        } => {
            let config = MastConfig::default().with_db_path(&db);
            let mut mast = Mast::open(config)?;

            let source_tier: Tier = source_tier.parse().map_err(|e: String| e)?;
            let target_tier: Tier = target_tier.parse().map_err(|e: String| e)?;

            let strategy = match strategy.as_str() {
                "age" => CompactionStrategy::Age {
                    older_than_secs: older_than,
                },
                "access_decay" => CompactionStrategy::AccessDecay,
                "hybrid" => CompactionStrategy::Hybrid,
                other => return Err(format!("unknown strategy: {other}").into()),
            };

            let compactor = MockCompactor;
            let embedder = MockEmbedder::new(0);

            let request = CompactRequest {
                collection,
                source_tier,
                target_tier,
                strategy,
                max_batch_size,
                dry_run,
            };

            let result = mast.compact(request, &compactor, &embedder).await?;

            if dry_run {
                println!(
                    "Dry run: {} candidate memories in '{}'",
                    result.source_count, result.collection
                );
                for id in &result.source_memory_ids {
                    println!("  {id}");
                }
            } else {
                println!(
                    "Compacted {} memories into {} new memories in '{}'",
                    result.compacted_count,
                    result.new_memory_ids.len(),
                    result.collection
                );
            }
        }

        Commands::Snapshot {
            db,
            collection,
            output,
        } => {
            let config = MastConfig::default().with_db_path(&db);
            let mast = Mast::open(config)?;

            let mut file = std::fs::File::create(&output)?;
            let result = mast.snapshot(&collection, &mut file)?;

            println!(
                "Snapshot '{}': {} memories, {} edges -> {}",
                result.collection,
                result.memory_count,
                result.edge_count,
                output.display(),
            );
        }

        Commands::Restore { db, input, merge } => {
            let config = MastConfig::default().with_db_path(&db);
            let mut mast = Mast::open(config)?;

            let file = std::fs::File::open(&input)?;
            let mut reader = std::io::BufReader::new(file);
            let result = mast.restore(&mut reader, merge)?;

            println!(
                "Restored '{}': {} memories, {} edges",
                result.collection, result.memory_count, result.edge_count,
            );
            if result.skipped_count > 0 {
                println!("  skipped {} duplicates", result.skipped_count);
            }
            if !result.warnings.is_empty() {
                for w in &result.warnings {
                    eprintln!("  warning: {w}");
                }
            }
            mast.close()?;
        }

        Commands::Relate {
            db,
            collection,
            source,
            target,
            relation,
            weight,
        } => {
            let config = MastConfig::default().with_db_path(&db);
            let mast = Mast::open(config)?;

            let request = RelateRequest {
                collection,
                source: source.clone(),
                target: target.clone(),
                relation: relation.clone(),
                weight,
                memory_ids: vec![],
            };

            let edge_id = mast.relate(request)?;
            println!(
                "Edge {edge_id}: {source} --[{relation}]--> {target} (weight={weight})"
            );
        }

        Commands::Traverse {
            db,
            collection,
            start,
            max_depth,
            relation,
            min_weight,
        } => {
            let config = MastConfig::default().with_db_path(&db);
            let mast = Mast::open(config)?;

            let request = TraverseRequest {
                collection,
                start,
                max_depth,
                relation_filter: relation,
                min_weight,
            };

            let edges = mast.traverse(request)?;

            if edges.is_empty() {
                println!("No edges found.");
            } else {
                for edge in &edges {
                    println!(
                        "{} --[{}]--> {} (weight={:.2})",
                        edge.source, edge.relation, edge.target, edge.weight
                    );
                }
                println!("\nTotal edges: {}", edges.len());
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_metadata ---

    #[test]
    fn parse_metadata_valid() {
        let (k, v) = parse_metadata("key=value").unwrap();
        assert_eq!(k, "key");
        assert_eq!(v, "value");
    }

    #[test]
    fn parse_metadata_value_with_equals() {
        let (k, v) = parse_metadata("url=https://example.com?a=1&b=2").unwrap();
        assert_eq!(k, "url");
        assert_eq!(v, "https://example.com?a=1&b=2");
    }

    #[test]
    fn parse_metadata_empty_value() {
        let (k, v) = parse_metadata("key=").unwrap();
        assert_eq!(k, "key");
        assert_eq!(v, "");
    }

    #[test]
    fn parse_metadata_empty_key() {
        let (k, v) = parse_metadata("=value").unwrap();
        assert_eq!(k, "");
        assert_eq!(v, "value");
    }

    #[test]
    fn parse_metadata_no_equals() {
        let err = parse_metadata("noequalssign").unwrap_err();
        assert!(err.contains("no '=' found"), "expected '=' error in: {err}");
    }

    #[test]
    fn parse_metadata_just_equals() {
        let (k, v) = parse_metadata("=").unwrap();
        assert_eq!(k, "");
        assert_eq!(v, "");
    }

    // --- parse_embedding ---

    #[test]
    fn parse_embedding_valid() {
        let v = parse_embedding("0.1,0.2,0.3").unwrap();
        assert_eq!(v, vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn parse_embedding_single_value() {
        let v = parse_embedding("1.5").unwrap();
        assert_eq!(v, vec![1.5]);
    }

    #[test]
    fn parse_embedding_with_spaces() {
        let v = parse_embedding(" 0.1 , 0.2 , 0.3 ").unwrap();
        assert_eq!(v, vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn parse_embedding_negative_values() {
        let v = parse_embedding("-0.5,0.5,-1.0").unwrap();
        assert_eq!(v, vec![-0.5, 0.5, -1.0]);
    }

    #[test]
    fn parse_embedding_invalid_float() {
        let err = parse_embedding("0.1,abc,0.3").unwrap_err();
        assert!(err.contains("invalid float"), "expected float error in: {err}");
    }

    #[test]
    fn parse_embedding_trailing_comma() {
        let err = parse_embedding("0.1,0.2,").unwrap_err();
        assert!(err.contains("invalid float"), "expected float error in: {err}");
    }

    #[test]
    fn parse_embedding_empty_string() {
        let err = parse_embedding("").unwrap_err();
        assert!(err.contains("invalid float"), "expected float error in: {err}");
    }

    // --- CLI argument parsing ---

    #[test]
    fn cli_store_requires_db() {
        let result = Cli::try_parse_from(["mast", "store", "--collection", "c", "--content", "x"]);
        assert!(result.is_err());
    }

    #[test]
    fn cli_store_requires_collection() {
        let result = Cli::try_parse_from(["mast", "store", "--db", "test.mast", "--content", "x"]);
        assert!(result.is_err());
    }

    #[test]
    fn cli_store_requires_content() {
        let result =
            Cli::try_parse_from(["mast", "store", "--db", "test.mast", "--collection", "c"]);
        assert!(result.is_err());
    }

    #[test]
    fn cli_recall_requires_db() {
        let result = Cli::try_parse_from([
            "mast",
            "recall",
            "--collection",
            "c",
            "--query-vec",
            "0.1,0.2",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn cli_store_default_tier() {
        let cli = Cli::try_parse_from([
            "mast",
            "store",
            "--db",
            "test.mast",
            "--collection",
            "c",
            "--content",
            "hello",
        ])
        .unwrap();
        match cli.command {
            Commands::Store { tier, .. } => assert_eq!(tier, "active"),
            _ => panic!("expected Store"),
        }
    }

    #[test]
    fn cli_recall_default_limit() {
        let cli = Cli::try_parse_from([
            "mast",
            "recall",
            "--db",
            "test.mast",
            "--collection",
            "c",
            "--query-vec",
            "0.1",
        ])
        .unwrap();
        match cli.command {
            Commands::Recall { limit, .. } => assert_eq!(limit, 10),
            _ => panic!("expected Recall"),
        }
    }

    #[test]
    fn cli_store_multiple_metadata() {
        let cli = Cli::try_parse_from([
            "mast",
            "store",
            "--db",
            "t.mast",
            "--collection",
            "c",
            "--content",
            "x",
            "--metadata",
            "a=1",
            "--metadata",
            "b=2",
        ])
        .unwrap();
        match cli.command {
            Commands::Store { metadata, .. } => {
                assert_eq!(metadata.len(), 2);
                assert!(metadata.contains(&("a".into(), "1".into())));
                assert!(metadata.contains(&("b".into(), "2".into())));
            }
            _ => panic!("expected Store"),
        }
    }

    #[test]
    fn cli_no_subcommand() {
        let result = Cli::try_parse_from(["mast"]);
        assert!(result.is_err());
    }

    #[test]
    fn cli_delete_parses() {
        let cli = Cli::try_parse_from([
            "mast", "delete", "--db", "t.mast", "--collection", "c", "--id", "01H0000000000000000000000",
        ])
        .unwrap();
        match cli.command {
            Commands::Delete { collection, .. } => assert_eq!(collection, "c"),
            _ => panic!("expected Delete"),
        }
    }

    #[test]
    fn cli_list_parses() {
        let cli = Cli::try_parse_from([
            "mast", "list", "--db", "t.mast", "--collection", "c",
        ])
        .unwrap();
        match cli.command {
            Commands::List { collection, .. } => assert_eq!(collection, "c"),
            _ => panic!("expected List"),
        }
    }

    #[test]
    fn cli_vacuum_parses() {
        let cli = Cli::try_parse_from([
            "mast", "vacuum", "--db", "t.mast", "--collection", "c",
        ])
        .unwrap();
        match cli.command {
            Commands::Vacuum { collection, .. } => assert_eq!(collection, "c"),
            _ => panic!("expected Vacuum"),
        }
    }
}
