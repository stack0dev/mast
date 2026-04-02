use redb::TableDefinition;

/// (collection, ulid_bytes) -> versioned-serialized Memory
pub const MEMORIES: TableDefinition<(&str, &[u8]), &[u8]> =
    TableDefinition::new("memories");

/// collection_name -> bincode-serialized CollectionInfo
pub const COLLECTIONS: TableDefinition<&str, &[u8]> =
    TableDefinition::new("collections");

/// collection_name -> next u64 sequential key for usearch
pub const SEQ: TableDefinition<&str, u64> =
    TableDefinition::new("seq");

/// (collection, ulid_bytes) -> u64 sequential key
pub const ULID_TO_SEQ: TableDefinition<(&str, &[u8]), u64> =
    TableDefinition::new("ulid_to_seq");

/// (collection, u64_be_bytes) -> ulid_bytes
pub const SEQ_TO_ULID: TableDefinition<(&str, &[u8]), &[u8]> =
    TableDefinition::new("seq_to_ulid");

/// (collection, meta_key, meta_value) -> bincode-serialized Vec<[u8; 16]> (ULID bytes posting list)
pub const META_INDEX: TableDefinition<(&str, &str, &str), &[u8]> =
    TableDefinition::new("meta_index");

/// collection_name -> usearch serialized buffer
pub const VECTOR_INDEX: TableDefinition<&str, &[u8]> =
    TableDefinition::new("vector_index");

/// (collection, edge_id_bytes) -> bincode-serialized Edge
pub const EDGES: TableDefinition<(&str, &[u8]), &[u8]> =
    TableDefinition::new("edges");

/// (collection, source_entity, relation) -> bincode-serialized Vec<[u8; 16]> (edge ID posting list)
pub const EDGE_INDEX_FWD: TableDefinition<(&str, &str, &str), &[u8]> =
    TableDefinition::new("edge_index_fwd");

/// (collection, target_entity, relation) -> bincode-serialized Vec<[u8; 16]> (edge ID posting list)
pub const EDGE_INDEX_REV: TableDefinition<(&str, &str, &str), &[u8]> =
    TableDefinition::new("edge_index_rev");

/// (collection, term) -> bincode-serialized PostingList
pub const FT_POSTING: TableDefinition<(&str, &str), &[u8]> =
    TableDefinition::new("ft_posting");

/// collection -> bincode-serialized (doc_count: u64, total_tokens: u64)
pub const FT_STATS: TableDefinition<&str, &[u8]> =
    TableDefinition::new("ft_stats");
