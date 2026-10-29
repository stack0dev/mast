use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ulid::Ulid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Tier {
    Core,
    #[default]
    Active,
    Background,
    Archive,
}

impl std::fmt::Display for Tier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Tier::Core => write!(f, "core"),
            Tier::Active => write!(f, "active"),
            Tier::Background => write!(f, "background"),
            Tier::Archive => write!(f, "archive"),
        }
    }
}

impl std::str::FromStr for Tier {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "core" => Ok(Tier::Core),
            "active" => Ok(Tier::Active),
            "background" => Ok(Tier::Background),
            "archive" => Ok(Tier::Archive),
            _ => Err(format!("invalid tier: {s}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: Ulid,
    pub collection: String,
    pub content: String,
    pub embedding: Vec<f32>,
    pub metadata: HashMap<String, String>,
    pub tier: Tier,
    pub created_at: i64,
    pub access_count: u64,
    pub last_accessed: i64,
    pub ttl: Option<i64>,
    pub compacted_at: Option<i64>,
    pub parent_ids: Vec<Ulid>,
}

#[derive(Debug, Clone)]
pub struct StoreRequest {
    pub collection: String,
    pub content: String,
    pub embedding: Option<Vec<f32>>,
    pub metadata: HashMap<String, String>,
    pub tier: Tier,
    pub ttl: Option<i64>,
    /// When the memory happened, in unix seconds. `None` = now. Set it when
    /// importing history so time decay, TTL and temporal recall use the real time.
    pub created_at: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub enum SearchMode {
    #[default]
    Vector,
    FullText,
    Hybrid {
        vector_weight: f32,
        text_weight: f32,
    },
}

impl std::fmt::Display for SearchMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SearchMode::Vector => write!(f, "vector"),
            SearchMode::FullText => write!(f, "fulltext"),
            SearchMode::Hybrid { .. } => write!(f, "hybrid"),
        }
    }
}

impl std::str::FromStr for SearchMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "vector" => Ok(SearchMode::Vector),
            "fulltext" => Ok(SearchMode::FullText),
            "hybrid" => Ok(SearchMode::Hybrid {
                vector_weight: 0.5,
                text_weight: 0.5,
            }),
            _ => Err(format!("invalid search mode: {s}")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RecallRequest {
    pub collection: String,
    pub query: Option<String>,
    pub query_embedding: Option<Vec<f32>>,
    pub text_query: Option<String>,
    pub search_mode: SearchMode,
    pub limit: usize,
    pub filter: Option<MetadataFilter>,
    pub tier_weights: Option<TierWeights>,
    pub time_decay: Option<TimeDecay>,
    /// When the question is asked, in unix seconds. `None` = now. Time decay
    /// and relative phrases in the query ("two weeks ago") resolve against it.
    pub reference_time: Option<i64>,
    /// Boost memories created near this time. `None` = parse a relative time
    /// phrase from the query text, if there is one.
    pub time_anchor: Option<TimeAnchor>,
}

/// A point in time that recall favours: memories created within `window_secs`
/// of `target` get a score boost that falls off linearly with distance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeAnchor {
    pub target: i64,
    pub window_secs: i64,
}

#[derive(Debug, Clone)]
pub struct RecallResult {
    pub memory: Memory,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionInfo {
    pub name: String,
    pub dimensions: usize,
    pub memory_count: u64,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub enum MetadataFilter {
    Eq(String, String),
    In(String, Vec<String>),
    And(Vec<MetadataFilter>),
    Or(Vec<MetadataFilter>),
}

#[derive(Debug, Clone, Copy)]
pub struct TierWeights {
    pub core: f32,
    pub active: f32,
    pub background: f32,
    pub archive: f32,
}

impl Default for TierWeights {
    fn default() -> Self {
        Self {
            core: 1.0,
            active: 0.9,
            background: 0.7,
            archive: 0.5,
        }
    }
}

impl TierWeights {
    pub fn weight_for(&self, tier: Tier) -> f32 {
        match tier {
            Tier::Core => self.core,
            Tier::Active => self.active,
            Tier::Background => self.background,
            Tier::Archive => self.archive,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TimeDecay {
    pub half_life_secs: i64,
}

#[derive(Debug, Clone)]
pub struct BatchStoreRequest {
    pub collection: String,
    pub items: Vec<BatchStoreItem>,
}

#[derive(Debug, Clone)]
pub struct BatchStoreItem {
    pub content: String,
    pub embedding: Option<Vec<f32>>,
    pub metadata: HashMap<String, String>,
    pub tier: Tier,
    pub ttl: Option<i64>,
    /// When the memory happened, in unix seconds. `None` = now.
    pub created_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub id: Ulid,
    pub collection: String,
    pub source: String,
    pub target: String,
    pub relation: String,
    pub weight: f32,
    pub memory_ids: Vec<Ulid>,
    pub created_at: i64,
    pub updated_at: i64,
    /// Start of the time the fact is true, in unix seconds. `None` = always.
    #[serde(default)]
    pub valid_from: Option<i64>,
    /// End of the time the fact is true, in unix seconds, exclusive. `None` = still true.
    #[serde(default)]
    pub valid_to: Option<i64>,
}

impl Edge {
    /// True when the fact holds at `at`. The interval is half-open: `[valid_from, valid_to)`.
    pub fn is_valid_at(&self, at: i64) -> bool {
        self.valid_from.is_none_or(|from| from <= at) && self.valid_to.is_none_or(|to| at < to)
    }
}

#[derive(Debug, Clone)]
pub struct RelateRequest {
    pub collection: String,
    pub source: String,
    pub target: String,
    pub relation: String,
    pub weight: f32,
    pub memory_ids: Vec<Ulid>,
    pub valid_from: Option<i64>,
    pub valid_to: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct InvalidateRequest {
    pub collection: String,
    pub source: String,
    pub target: String,
    pub relation: String,
    /// When the fact stopped being true, in unix seconds. `None` = now.
    pub ended_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct TraverseRequest {
    pub collection: String,
    pub start: String,
    pub max_depth: usize,
    pub relation_filter: Option<String>,
    pub min_weight: Option<f32>,
    /// Follow only edges valid at this time, in unix seconds. `None` = all edges.
    pub as_of: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct Graph {
    pub collection: String,
    pub edges: Vec<Edge>,
}

#[derive(Debug, Clone)]
pub struct CompactRequest {
    pub collection: String,
    pub source_tier: Tier,
    pub target_tier: Tier,
    pub strategy: CompactionStrategy,
    pub max_batch_size: usize,
    pub dry_run: bool,
}

#[derive(Debug, Clone)]
pub enum CompactionStrategy {
    Age { older_than_secs: i64 },
    AccessDecay,
    Hybrid,
    Manual { ids: Vec<Ulid> },
}

#[derive(Debug, Clone)]
pub struct CompactResult {
    pub collection: String,
    pub source_count: usize,
    pub compacted_count: usize,
    pub new_memory_ids: Vec<Ulid>,
    pub source_memory_ids: Vec<Ulid>,
}

#[derive(Debug, Clone)]
pub struct VacuumResult {
    pub collection: String,
    pub expired_count: usize,
}

#[derive(Debug, Clone)]
pub struct SnapshotResult {
    pub collection: String,
    pub memory_count: u64,
    pub edge_count: u64,
}

#[derive(Debug, Clone)]
pub struct RestoreResult {
    pub collection: String,
    pub memory_count: u64,
    pub edge_count: u64,
    pub skipped_count: u64,
    pub warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_bincode_roundtrip() {
        let memory = Memory {
            id: Ulid::new(),
            collection: "test".into(),
            content: "hello world".into(),
            embedding: vec![0.1, 0.2, 0.3],
            metadata: HashMap::from([("key".into(), "val".into())]),
            tier: Tier::Active,
            created_at: 1000,
            access_count: 5,
            last_accessed: 2000,
            ttl: Some(3600),
            compacted_at: None,
            parent_ids: vec![],
        };
        let bytes = bincode::serialize(&memory).unwrap();
        let decoded: Memory = bincode::deserialize(&bytes).unwrap();
        assert_eq!(decoded.id, memory.id);
        assert_eq!(decoded.collection, memory.collection);
        assert_eq!(decoded.content, memory.content);
        assert_eq!(decoded.embedding, memory.embedding);
        assert_eq!(decoded.metadata, memory.metadata);
        assert_eq!(decoded.tier, memory.tier);
        assert_eq!(decoded.created_at, memory.created_at);
        assert_eq!(decoded.access_count, memory.access_count);
        assert_eq!(decoded.last_accessed, memory.last_accessed);
        assert_eq!(decoded.ttl, memory.ttl);
        assert_eq!(decoded.compacted_at, memory.compacted_at);
        assert_eq!(decoded.parent_ids, memory.parent_ids);
    }

    #[test]
    fn tier_default_is_active() {
        assert_eq!(Tier::default(), Tier::Active);
    }

    #[test]
    fn tier_display() {
        assert_eq!(Tier::Core.to_string(), "core");
        assert_eq!(Tier::Active.to_string(), "active");
        assert_eq!(Tier::Background.to_string(), "background");
        assert_eq!(Tier::Archive.to_string(), "archive");
    }

    #[test]
    fn tier_from_str() {
        assert_eq!("core".parse::<Tier>().unwrap(), Tier::Core);
        assert_eq!("ACTIVE".parse::<Tier>().unwrap(), Tier::Active);
        assert_eq!("Background".parse::<Tier>().unwrap(), Tier::Background);
        assert_eq!("archive".parse::<Tier>().unwrap(), Tier::Archive);
        assert!("invalid".parse::<Tier>().is_err());
    }

    #[test]
    fn tier_bincode_roundtrip() {
        for tier in [Tier::Core, Tier::Active, Tier::Background, Tier::Archive] {
            let bytes = bincode::serialize(&tier).unwrap();
            let decoded: Tier = bincode::deserialize(&bytes).unwrap();
            assert_eq!(decoded, tier);
        }
    }

    #[test]
    fn collection_info_bincode_roundtrip() {
        let info = CollectionInfo {
            name: "test_col".into(),
            dimensions: 1536,
            memory_count: 42,
            created_at: 12345,
        };
        let bytes = bincode::serialize(&info).unwrap();
        let decoded: CollectionInfo = bincode::deserialize(&bytes).unwrap();
        assert_eq!(decoded.name, info.name);
        assert_eq!(decoded.dimensions, info.dimensions);
        assert_eq!(decoded.memory_count, info.memory_count);
        assert_eq!(decoded.created_at, info.created_at);
    }

    #[test]
    fn store_request_fields() {
        let req = StoreRequest {
            collection: "col".into(),
            content: "text".into(),
            embedding: Some(vec![1.0, 2.0]),
            metadata: HashMap::new(),
            tier: Tier::Core,
            ttl: None,
            created_at: None,
        };
        assert_eq!(req.collection, "col");
        assert_eq!(req.embedding.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn recall_request_fields() {
        let req = RecallRequest {
            collection: "col".into(),
            query: Some("search".into()),
            query_embedding: None,
            text_query: None,
            search_mode: SearchMode::Vector,
            limit: 5,
            filter: Some(MetadataFilter::Eq("k".into(), "v".into())),
            tier_weights: None,
            time_decay: None,
            reference_time: None,
            time_anchor: None,
        };
        assert_eq!(req.limit, 5);
        assert!(req.filter.is_some());
    }

    #[test]
    fn search_mode_default() {
        assert!(matches!(SearchMode::default(), SearchMode::Vector));
    }

    #[test]
    fn search_mode_display() {
        assert_eq!(SearchMode::Vector.to_string(), "vector");
        assert_eq!(SearchMode::FullText.to_string(), "fulltext");
        assert_eq!(
            SearchMode::Hybrid {
                vector_weight: 0.5,
                text_weight: 0.5
            }
            .to_string(),
            "hybrid"
        );
    }

    #[test]
    fn search_mode_from_str() {
        assert!(matches!("vector".parse::<SearchMode>().unwrap(), SearchMode::Vector));
        assert!(matches!("fulltext".parse::<SearchMode>().unwrap(), SearchMode::FullText));
        assert!(matches!("hybrid".parse::<SearchMode>().unwrap(), SearchMode::Hybrid { .. }));
        assert!("invalid".parse::<SearchMode>().is_err());
    }

    #[test]
    fn tier_weights_default() {
        let tw = TierWeights::default();
        assert_eq!(tw.core, 1.0);
        assert_eq!(tw.active, 0.9);
        assert_eq!(tw.background, 0.7);
        assert_eq!(tw.archive, 0.5);
    }

    #[test]
    fn tier_weights_weight_for() {
        let tw = TierWeights::default();
        assert_eq!(tw.weight_for(Tier::Core), 1.0);
        assert_eq!(tw.weight_for(Tier::Active), 0.9);
        assert_eq!(tw.weight_for(Tier::Background), 0.7);
        assert_eq!(tw.weight_for(Tier::Archive), 0.5);
    }

    #[test]
    fn edge_bincode_roundtrip() {
        let edge = Edge {
            id: Ulid::new(),
            collection: "test".into(),
            source: "alice".into(),
            target: "bob".into(),
            relation: "knows".into(),
            weight: 0.8,
            memory_ids: vec![Ulid::new()],
            created_at: 1000,
            updated_at: 2000,
            valid_from: Some(1000),
            valid_to: None,
        };
        let bytes = bincode::serialize(&edge).unwrap();
        let decoded: Edge = bincode::deserialize(&bytes).unwrap();
        assert_eq!(decoded.id, edge.id);
        assert_eq!(decoded.source, edge.source);
        assert_eq!(decoded.target, edge.target);
        assert_eq!(decoded.relation, edge.relation);
        assert_eq!(decoded.valid_from, edge.valid_from);
        assert_eq!(decoded.valid_to, edge.valid_to);
    }

    #[test]
    fn edge_validity_is_half_open() {
        let edge = Edge {
            id: Ulid::new(),
            collection: "test".into(),
            source: "alice".into(),
            target: "acme".into(),
            relation: "works_at".into(),
            weight: 1.0,
            memory_ids: vec![],
            created_at: 0,
            updated_at: 0,
            valid_from: Some(100),
            valid_to: Some(200),
        };
        assert!(!edge.is_valid_at(99));
        assert!(edge.is_valid_at(100));
        assert!(edge.is_valid_at(199));
        assert!(!edge.is_valid_at(200));

        let open = Edge { valid_from: None, valid_to: None, ..edge };
        assert!(open.is_valid_at(i64::MIN));
        assert!(open.is_valid_at(i64::MAX));
    }
}
