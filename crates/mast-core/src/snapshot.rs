use crate::error::MastError;
use crate::types::{Edge, Memory, RestoreResult, SnapshotResult};
use crate::Mast;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};

#[derive(Serialize, Deserialize)]
struct SnapshotHeader {
    mast_snapshot: String,
    collection: String,
    memory_count: u64,
    edge_count: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
enum SnapshotLine {
    #[serde(rename = "memory")]
    Memory(Memory),
    #[serde(rename = "edge")]
    Edge(Edge),
}

impl Mast {
    pub fn snapshot(
        &self,
        collection: &str,
        writer: &mut dyn Write,
    ) -> Result<SnapshotResult, MastError> {
        let memories = self.storage.list_memories(collection)?;
        let edges = self.storage.list_edges(collection)?;

        let header = SnapshotHeader {
            mast_snapshot: "1.0".to_string(),
            collection: collection.to_string(),
            memory_count: memories.len() as u64,
            edge_count: edges.len() as u64,
        };

        serde_json::to_writer(&mut *writer, &header)
            .map_err(|e| MastError::Snapshot(format!("failed to write header: {e}")))?;
        writer
            .write_all(b"\n")
            .map_err(|e| MastError::Snapshot(format!("write error: {e}")))?;

        for memory in &memories {
            let line = SnapshotLine::Memory(memory.clone());
            serde_json::to_writer(&mut *writer, &line)
                .map_err(|e| MastError::Snapshot(format!("failed to write memory: {e}")))?;
            writer
                .write_all(b"\n")
                .map_err(|e| MastError::Snapshot(format!("write error: {e}")))?;
        }

        for edge in &edges {
            let line = SnapshotLine::Edge(edge.clone());
            serde_json::to_writer(&mut *writer, &line)
                .map_err(|e| MastError::Snapshot(format!("failed to write edge: {e}")))?;
            writer
                .write_all(b"\n")
                .map_err(|e| MastError::Snapshot(format!("write error: {e}")))?;
        }

        Ok(SnapshotResult {
            collection: collection.to_string(),
            memory_count: memories.len() as u64,
            edge_count: edges.len() as u64,
        })
    }

    pub fn restore(
        &mut self,
        reader: &mut dyn BufRead,
        merge: bool,
    ) -> Result<RestoreResult, MastError> {
        use crate::storage::Storage;

        let mut line_buf = String::new();

        // Read header
        reader
            .read_line(&mut line_buf)
            .map_err(|e| MastError::Snapshot(format!("failed to read header: {e}")))?;
        let header: SnapshotHeader = serde_json::from_str(line_buf.trim())
            .map_err(|e| MastError::Snapshot(format!("invalid header: {e}")))?;

        if header.mast_snapshot != "1.0" {
            return Err(MastError::Snapshot(format!(
                "unsupported snapshot version: {}",
                header.mast_snapshot
            )));
        }

        // Check collection doesn't already exist (unless merging)
        if !merge {
            let collections = self.storage.list_collections()?;
            if collections.iter().any(|c| c.name == header.collection) {
                return Err(MastError::Snapshot(format!(
                    "collection '{}' already exists; restore requires an empty collection",
                    header.collection
                )));
            }
        }

        let mut skipped_count = 0u64;
        let mut warnings = Vec::new();

        // Parse all lines into memories and edges first
        let mut memories = Vec::new();
        let mut edges = Vec::new();

        loop {
            line_buf.clear();
            let bytes_read = reader
                .read_line(&mut line_buf)
                .map_err(|e| MastError::Snapshot(format!("read error: {e}")))?;
            if bytes_read == 0 {
                break;
            }

            let trimmed = line_buf.trim();
            if trimmed.is_empty() {
                continue;
            }

            match serde_json::from_str::<SnapshotLine>(trimmed) {
                Ok(SnapshotLine::Memory(memory)) => {
                    if merge {
                        if self
                            .storage
                            .read_memory(&memory.collection, &memory.id)?
                            .is_some()
                        {
                            skipped_count += 1;
                            continue;
                        }
                    }
                    memories.push(memory);
                }
                Ok(SnapshotLine::Edge(edge)) => {
                    if merge
                        && self.storage.edge_exists(
                            &edge.collection,
                            &edge.source,
                            &edge.target,
                            &edge.relation,
                        )?
                    {
                        skipped_count += 1;
                        continue;
                    }
                    edges.push(edge);
                }
                Err(e) => {
                    warnings.push(format!("skipping invalid line: {e}"));
                }
            }
        }

        let memory_count = memories.len() as u64;
        let edge_count = edges.len() as u64;

        // Process memories in chunks — single txn per chunk
        const CHUNK_SIZE: usize = 100;
        for chunk in memories.chunks(CHUNK_SIZE) {
            let txn = self.storage.begin_write_txn()?;

            for memory in chunk {
                let seq = Storage::write_memory_in_txn(&txn, memory)?;

                if !memory.metadata.is_empty() {
                    for (key, value) in &memory.metadata {
                        Storage::append_meta_in_txn(
                            &txn,
                            &memory.collection,
                            key,
                            value,
                            &memory.id,
                        )?;
                    }
                }

                Storage::ft_insert_in_txn(&txn, &memory.collection, &memory.id, &memory.content)?;

                let dims = memory.embedding.len();
                let index = self
                    .index_manager
                    .get_or_create(&memory.collection, dims)?;
                if let Err(e) = index.add(seq, &memory.embedding) {
                    warnings.push(format!("failed to index memory {}: {e}", memory.id));
                }
            }

            txn.commit()?;
        }

        // Process edges in a single transaction
        if !edges.is_empty() {
            let txn = self.storage.begin_write_txn()?;
            for edge in &edges {
                Storage::write_edge_in_txn(&txn, edge)?;
            }
            txn.commit()?;
        }

        // Flush indexes
        self.index_manager
            .flush_collection(&header.collection, &self.storage)?;

        Ok(RestoreResult {
            collection: header.collection,
            memory_count,
            edge_count,
            skipped_count,
            warnings,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MastConfig;
    use crate::embed::MockEmbedder;
    use crate::types::*;
    use std::collections::HashMap;
    use std::io::Cursor;

    fn temp_config() -> (tempfile::TempDir, MastConfig) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.mast");
        let config = MastConfig::default().with_db_path(db_path);
        (dir, config)
    }

    #[tokio::test]
    async fn snapshot_and_restore_roundtrip() {
        let (_dir1, config1) = temp_config();
        let embedder = MockEmbedder::new(32);
        let mut mast1 = Mast::open(config1).unwrap();

        // Store some memories
        for i in 0..3 {
            mast1
                .store(
                    StoreRequest {
                        collection: "test".into(),
                        content: format!("memory {i}"),
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

        // Add an edge
        mast1
            .relate(RelateRequest {
                collection: "test".into(),
                source: "alice".into(),
                target: "bob".into(),
                relation: "knows".into(),
                weight: 0.9,
                memory_ids: vec![],
            })
            .unwrap();

        // Snapshot
        let mut buf = Vec::new();
        let snap_result = mast1.snapshot("test", &mut buf).unwrap();
        assert_eq!(snap_result.memory_count, 3);
        assert_eq!(snap_result.edge_count, 1);
        drop(mast1);

        // Restore into a new database
        let (_dir2, config2) = temp_config();
        let mut mast2 = Mast::open(config2).unwrap();

        let mut cursor = Cursor::new(&buf);
        let restore_result = mast2.restore(&mut cursor, false).unwrap();
        assert_eq!(restore_result.collection, "test");
        assert_eq!(restore_result.memory_count, 3);
        assert_eq!(restore_result.edge_count, 1);
        assert_eq!(restore_result.skipped_count, 0);

        // Verify memories are searchable
        let memories = mast2.list("test").unwrap();
        assert_eq!(memories.len(), 3);

        // Verify edges
        let graph = mast2.subgraph("test").unwrap();
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.edges[0].source, "alice");
        assert_eq!(graph.edges[0].target, "bob");

        // Verify vector search works
        let results = mast2
            .recall(
                RecallRequest {
                    collection: "test".into(),
                    query: Some("memory".into()),
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
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn restore_fails_if_collection_exists() {
        let (_dir, config) = temp_config();
        let mut mast = Mast::open(config).unwrap();

        // Create a collection by writing a memory
        let memory = Memory {
            id: ulid::Ulid::new(),
            collection: "test".into(),
            content: "hello".into(),
            embedding: vec![0.1, 0.2, 0.3],
            metadata: HashMap::new(),
            tier: Tier::Active,
            created_at: 1000,
            access_count: 0,
            last_accessed: 1000,
            ttl: None,
            compacted_at: None,
            parent_ids: vec![],
        };
        mast.storage.write_memory(&memory).unwrap();

        let snapshot_data = r#"{"mast_snapshot":"1.0","collection":"test","memory_count":0,"edge_count":0}
"#;
        let mut cursor = Cursor::new(snapshot_data.as_bytes());
        let result = mast.restore(&mut cursor, false);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("already exists"), "got: {err}");
    }

    #[test]
    fn restore_fails_on_bad_version() {
        let (_dir, config) = temp_config();
        let mut mast = Mast::open(config).unwrap();

        let snapshot_data = r#"{"mast_snapshot":"99.0","collection":"test","memory_count":0,"edge_count":0}
"#;
        let mut cursor = Cursor::new(snapshot_data.as_bytes());
        let result = mast.restore(&mut cursor, false);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("unsupported snapshot version"), "got: {err}");
    }

    #[test]
    fn snapshot_empty_collection() {
        let (_dir, config) = temp_config();
        let mast = Mast::open(config).unwrap();

        let mut buf = Vec::new();
        let result = mast.snapshot("empty_col", &mut buf).unwrap();
        assert_eq!(result.memory_count, 0);
        assert_eq!(result.edge_count, 0);
    }

    #[tokio::test]
    async fn merge_into_empty_collection() {
        let (_dir1, config1) = temp_config();
        let embedder = MockEmbedder::new(32);
        let mut mast1 = Mast::open(config1).unwrap();

        mast1
            .store(
                StoreRequest {
                    collection: "test".into(),
                    content: "hello".into(),
                    embedding: None,
                    metadata: HashMap::new(),
                    tier: Tier::Active,
                    ttl: None,
                },
                &embedder,
            )
            .await
            .unwrap();

        let mut buf = Vec::new();
        mast1.snapshot("test", &mut buf).unwrap();
        drop(mast1);

        let (_dir2, config2) = temp_config();
        let mut mast2 = Mast::open(config2).unwrap();
        let mut cursor = Cursor::new(&buf);
        let result = mast2.restore(&mut cursor, true).unwrap();
        assert_eq!(result.memory_count, 1);
        assert_eq!(result.skipped_count, 0);
    }

    #[tokio::test]
    async fn merge_skips_existing_memories() {
        let (_dir, config) = temp_config();
        let embedder = MockEmbedder::new(32);
        let mut mast = Mast::open(config).unwrap();

        let mem = mast
            .store(
                StoreRequest {
                    collection: "test".into(),
                    content: "original".into(),
                    embedding: None,
                    metadata: HashMap::new(),
                    tier: Tier::Active,
                    ttl: None,
                },
                &embedder,
            )
            .await
            .unwrap();

        // Snapshot
        let mut buf = Vec::new();
        mast.snapshot("test", &mut buf).unwrap();

        // Merge back into same db — memory already exists
        let mut cursor = Cursor::new(&buf);
        let result = mast.restore(&mut cursor, true).unwrap();
        assert_eq!(result.memory_count, 0);
        assert_eq!(result.skipped_count, 1);

        // Verify no duplicates
        let all = mast.list("test").unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, mem.id);
    }

    #[tokio::test]
    async fn merge_adds_new_memories() {
        let embedder = MockEmbedder::new(32);

        // DB1: store mem_a
        let (_dir1, config1) = temp_config();
        let mut mast1 = Mast::open(config1).unwrap();
        mast1
            .store(
                StoreRequest {
                    collection: "test".into(),
                    content: "mem_a".into(),
                    embedding: None,
                    metadata: HashMap::new(),
                    tier: Tier::Active,
                    ttl: None,
                },
                &embedder,
            )
            .await
            .unwrap();
        let mut buf1 = Vec::new();
        mast1.snapshot("test", &mut buf1).unwrap();
        drop(mast1);

        // DB2: store mem_b, then merge snapshot of mem_a
        let (_dir2, config2) = temp_config();
        let mut mast2 = Mast::open(config2).unwrap();
        mast2
            .store(
                StoreRequest {
                    collection: "test".into(),
                    content: "mem_b".into(),
                    embedding: None,
                    metadata: HashMap::new(),
                    tier: Tier::Active,
                    ttl: None,
                },
                &embedder,
            )
            .await
            .unwrap();

        let mut cursor = Cursor::new(&buf1);
        let result = mast2.restore(&mut cursor, true).unwrap();
        assert_eq!(result.memory_count, 1);
        assert_eq!(result.skipped_count, 0);

        let all = mast2.list("test").unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn merge_skips_existing_edges() {
        let (_dir, config) = temp_config();
        let mut mast = Mast::open(config).unwrap();

        mast.relate(RelateRequest {
            collection: "test".into(),
            source: "alice".into(),
            target: "bob".into(),
            relation: "knows".into(),
            weight: 1.0,
            memory_ids: vec![],
        })
        .unwrap();

        let mut buf = Vec::new();
        mast.snapshot("test", &mut buf).unwrap();

        // Merge back — edge already exists
        let mut cursor = Cursor::new(&buf);
        let result = mast.restore(&mut cursor, true).unwrap();
        assert_eq!(result.edge_count, 0);
        assert_eq!(result.skipped_count, 1);

        // No duplicate edges
        let graph = mast.subgraph("test").unwrap();
        assert_eq!(graph.edges.len(), 1);
    }
}
