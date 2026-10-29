use crate::error::MastError;
use crate::types::{Edge, Memory};

const VERSION_V1: u8 = 0x01;

/// V0 layout: the original Memory struct without ttl, compacted_at, parent_ids.
/// Stored as raw bincode with no version prefix.
#[derive(serde::Serialize, serde::Deserialize)]
struct MemoryV0 {
    id: ulid::Ulid,
    collection: String,
    content: String,
    embedding: Vec<f32>,
    metadata: std::collections::HashMap<String, String>,
    tier: crate::types::Tier,
    created_at: i64,
    access_count: u64,
    last_accessed: i64,
}

pub fn serialize_memory(m: &Memory) -> Result<Vec<u8>, MastError> {
    let payload = bincode::serialize(m)?;
    let mut buf = Vec::with_capacity(1 + payload.len());
    buf.push(VERSION_V1);
    buf.extend_from_slice(&payload);
    Ok(buf)
}

pub fn deserialize_memory(data: &[u8]) -> Result<Memory, MastError> {
    if data.is_empty() {
        return Err(MastError::Serialization("empty memory data".into()));
    }

    match data[0] {
        VERSION_V1 => {
            let memory: Memory = bincode::deserialize(&data[1..])?;
            Ok(memory)
        }
        _ => {
            // No recognized version byte — treat as v0 (raw bincode, original layout)
            let v0: MemoryV0 = bincode::deserialize(data)?;
            Ok(Memory {
                id: v0.id,
                collection: v0.collection,
                content: v0.content,
                embedding: v0.embedding,
                metadata: v0.metadata,
                tier: v0.tier,
                created_at: v0.created_at,
                access_count: v0.access_count,
                last_accessed: v0.last_accessed,
                ttl: None,
                compacted_at: None,
                parent_ids: vec![],
            })
        }
    }
}

/// V0 layout: the original Edge struct without valid_from, valid_to.
/// Stored as raw bincode with no version prefix.
#[derive(serde::Serialize, serde::Deserialize)]
struct EdgeV0 {
    id: ulid::Ulid,
    collection: String,
    source: String,
    target: String,
    relation: String,
    weight: f32,
    memory_ids: Vec<ulid::Ulid>,
    created_at: i64,
    updated_at: i64,
}

pub fn serialize_edge(e: &Edge) -> Result<Vec<u8>, MastError> {
    let payload = bincode::serialize(e)?;
    let mut buf = Vec::with_capacity(1 + payload.len());
    buf.push(VERSION_V1);
    buf.extend_from_slice(&payload);
    Ok(buf)
}

pub fn deserialize_edge(data: &[u8]) -> Result<Edge, MastError> {
    if data.is_empty() {
        return Err(MastError::Serialization("empty edge data".into()));
    }

    match data[0] {
        VERSION_V1 => {
            let edge: Edge = bincode::deserialize(&data[1..])?;
            Ok(edge)
        }
        _ => {
            // No recognized version byte — treat as v0 (raw bincode, original layout).
            // A v0 record starts with the ULID's string length (26), never 0x01.
            let v0: EdgeV0 = bincode::deserialize(data)?;
            Ok(Edge {
                id: v0.id,
                collection: v0.collection,
                source: v0.source,
                target: v0.target,
                relation: v0.relation,
                weight: v0.weight,
                memory_ids: v0.memory_ids,
                created_at: v0.created_at,
                updated_at: v0.updated_at,
                valid_from: None,
                valid_to: None,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Tier;
    use std::collections::HashMap;
    use ulid::Ulid;

    fn make_memory() -> Memory {
        Memory {
            id: Ulid::new(),
            collection: "test".into(),
            content: "hello".into(),
            embedding: vec![0.1, 0.2, 0.3],
            metadata: HashMap::from([("k".into(), "v".into())]),
            tier: Tier::Active,
            created_at: 1000,
            access_count: 5,
            last_accessed: 2000,
            ttl: Some(3600),
            compacted_at: None,
            parent_ids: vec![Ulid::new()],
        }
    }

    #[test]
    fn v1_roundtrip() {
        let mem = make_memory();
        let data = serialize_memory(&mem).unwrap();
        assert_eq!(data[0], VERSION_V1);
        let decoded = deserialize_memory(&data).unwrap();
        assert_eq!(decoded.id, mem.id);
        assert_eq!(decoded.ttl, mem.ttl);
        assert_eq!(decoded.parent_ids, mem.parent_ids);
    }

    #[test]
    fn v0_compat() {
        // Simulate v0 data: bincode of the old struct (no version prefix)
        let v0 = MemoryV0 {
            id: Ulid::new(),
            collection: "old".into(),
            content: "legacy".into(),
            embedding: vec![1.0, 2.0],
            metadata: HashMap::new(),
            tier: Tier::Core,
            created_at: 500,
            access_count: 0,
            last_accessed: 500,
        };
        let data = bincode::serialize(&v0).unwrap();

        let decoded = deserialize_memory(&data).unwrap();
        assert_eq!(decoded.id, v0.id);
        assert_eq!(decoded.content, "legacy");
        assert_eq!(decoded.ttl, None);
        assert_eq!(decoded.compacted_at, None);
        assert!(decoded.parent_ids.is_empty());
    }

    #[test]
    fn empty_data_errors() {
        assert!(deserialize_memory(&[]).is_err());
        assert!(deserialize_edge(&[]).is_err());
    }

    fn make_edge() -> Edge {
        Edge {
            id: Ulid::new(),
            collection: "test".into(),
            source: "alice".into(),
            target: "acme".into(),
            relation: "works_at".into(),
            weight: 0.7,
            memory_ids: vec![Ulid::new()],
            created_at: 1000,
            updated_at: 2000,
            valid_from: Some(1000),
            valid_to: Some(5000),
        }
    }

    #[test]
    fn edge_v1_roundtrip() {
        let edge = make_edge();
        let data = serialize_edge(&edge).unwrap();
        assert_eq!(data[0], VERSION_V1);
        let decoded = deserialize_edge(&data).unwrap();
        assert_eq!(decoded.id, edge.id);
        assert_eq!(decoded.weight, edge.weight);
        assert_eq!(decoded.memory_ids, edge.memory_ids);
        assert_eq!(decoded.valid_from, Some(1000));
        assert_eq!(decoded.valid_to, Some(5000));
    }

    #[test]
    fn edge_v0_compat() {
        let v0 = EdgeV0 {
            id: Ulid::new(),
            collection: "old".into(),
            source: "alice".into(),
            target: "bob".into(),
            relation: "knows".into(),
            weight: 1.0,
            memory_ids: vec![],
            created_at: 500,
            updated_at: 600,
        };
        let data = bincode::serialize(&v0).unwrap();
        assert_ne!(data[0], VERSION_V1);

        let decoded = deserialize_edge(&data).unwrap();
        assert_eq!(decoded.id, v0.id);
        assert_eq!(decoded.relation, "knows");
        assert_eq!(decoded.updated_at, 600);
        assert_eq!(decoded.valid_from, None);
        assert_eq!(decoded.valid_to, None);
    }
}
