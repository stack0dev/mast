pub mod compact;
pub mod config;
pub mod embed;
pub mod error;
pub mod index;
pub mod snapshot;
pub mod storage;
pub mod temporal;
pub mod types;

use compact::Compactor;
use config::MastConfig;
use embed::Embedder;
use error::MastError;
use index::fulltext::FullTextIndex;
use index::metadata::MetadataIndex;
use index::IndexManager;
use redb::ReadableTable;
use storage::Storage;
use types::*;
use ulid::Ulid;

use std::collections::{HashSet, VecDeque};

const ACCESS_STATS_FLUSH_THRESHOLD: usize = 100;

pub struct Mast {
    storage: Storage,
    index_manager: IndexManager,
    config: MastConfig,
    pending_access_updates: Vec<(String, Ulid, i64)>,
}

impl Drop for Mast {
    fn drop(&mut self) {
        if let Err(e) = self.flush_access_stats() {
            tracing::error!("failed to flush access stats on drop: {e}");
        }
        if let Err(e) = self.index_manager.flush_all(&self.storage) {
            tracing::error!("failed to flush indexes on drop: {e}");
        }
    }
}

impl Mast {
    pub fn open(config: MastConfig) -> Result<Self, MastError> {
        let storage = Storage::open_with_durability(&config.db_path, config.durability)?;
        let index_manager = IndexManager::open(&storage)?;
        Ok(Self {
            storage,
            index_manager,
            config,
            pending_access_updates: Vec::new(),
        })
    }

    pub async fn store(
        &mut self,
        request: StoreRequest,
        embedder: &dyn Embedder,
    ) -> Result<Memory, MastError> {
        let embedding = match request.embedding {
            Some(emb) => emb,
            None => embedder.embed(&request.content).await?,
        };

        if embedding.is_empty() {
            return Err(MastError::InvalidInput("embedding must not be empty".into()));
        }

        if let Some(idx) = self.index_manager.get(&request.collection) {
            if idx.dimensions() != embedding.len() {
                return Err(MastError::InvalidInput(format!(
                    "embedding dimensions {} do not match collection dimensions {}",
                    embedding.len(),
                    idx.dimensions(),
                )));
            }
        }

        let now = unix_now();

        let created_at = request.created_at.unwrap_or(now);
        let memory = Memory {
            id: Ulid::new(),
            collection: request.collection,
            content: request.content,
            embedding,
            metadata: request.metadata,
            tier: request.tier,
            created_at,
            access_count: 0,
            last_accessed: now,
            ttl: request.ttl,
            compacted_at: None,
            parent_ids: vec![],
        };

        // Single transaction for all storage + index writes
        let txn = self.storage.begin_write_txn()?;
        let seq = Storage::write_memory_in_txn(&txn, &memory)?;

        if !memory.metadata.is_empty() {
            for (key, value) in &memory.metadata {
                Storage::append_meta_in_txn(&txn, &memory.collection, key, value, &memory.id)?;
            }
        }

        Storage::ft_insert_in_txn(&txn, &memory.collection, &memory.id, &memory.content)?;

        // Add to in-memory vector index
        let dims = memory.embedding.len();
        let index = self.index_manager.get_or_create(&memory.collection, dims)?;
        if let Err(e) = index.add(seq, &memory.embedding) {
            // txn drops without commit = automatic rollback
            return Err(e);
        }

        // Commit — if this fails, rollback the in-memory vector
        match txn.commit() {
            Ok(()) => Ok(memory),
            Err(e) => {
                let _ = index.remove(seq);
                Err(MastError::from(e))
            }
        }
    }

    pub async fn store_batch(
        &mut self,
        request: BatchStoreRequest,
        embedder: &dyn Embedder,
    ) -> Result<Vec<Memory>, MastError> {
        if request.items.is_empty() {
            return Ok(vec![]);
        }

        // Collect texts that need embedding
        let mut texts_to_embed: Vec<String> = Vec::new();
        for item in &request.items {
            if item.embedding.is_none() {
                texts_to_embed.push(item.content.clone());
            }
        }

        // Batch embed
        let generated_embeddings = if !texts_to_embed.is_empty() {
            let embs = embedder.embed_batch(&texts_to_embed).await?;
            if embs.len() != texts_to_embed.len() {
                return Err(MastError::Embedder(format!(
                    "embed_batch returned {} vectors, expected {}",
                    embs.len(),
                    texts_to_embed.len()
                )));
            }
            embs
        } else {
            vec![]
        };

        let now = unix_now();

        // Build Memory objects
        let mut memories = Vec::with_capacity(request.items.len());
        let mut gen_idx = 0;
        for (i, item) in request.items.into_iter().enumerate() {
            let embedding = if let Some(emb) = item.embedding {
                emb
            } else {
                let emb = generated_embeddings[gen_idx].clone();
                gen_idx += 1;
                emb
            };

            if embedding.is_empty() {
                return Err(MastError::InvalidInput(
                    format!("embedding must not be empty for item {i}"),
                ));
            }

            if let Some(idx) = self.index_manager.get(&request.collection) {
                if idx.dimensions() != embedding.len() {
                    return Err(MastError::InvalidInput(format!(
                        "embedding dimensions {} do not match collection dimensions {} for item {i}",
                        embedding.len(),
                        idx.dimensions(),
                    )));
                }
            }

            memories.push(Memory {
                id: Ulid::new(),
                collection: request.collection.clone(),
                content: item.content,
                embedding,
                metadata: item.metadata,
                tier: item.tier,
                created_at: item.created_at.unwrap_or(now),
                access_count: 0,
                last_accessed: now,
                ttl: item.ttl,
                compacted_at: None,
                parent_ids: vec![],
            });
        }

        // Single transaction for all storage + metadata + fulltext writes
        let txn = self.storage.begin_write_txn()?;
        let seqs = Storage::write_batch_in_txn(&txn, &memories)?;

        // Pre-reserve usearch capacity for the entire batch
        {
            let dims = memories[0].embedding.len();
            let index = self
                .index_manager
                .get_or_create(&request.collection, dims)?;
            index.reserve_additional(memories.len())?;
        }

        // Add to in-memory vector indexes — track for rollback
        let mut vectors_added = 0usize;
        for (memory, &seq) in memories.iter().zip(seqs.iter()) {
            let dims = memory.embedding.len();
            let index = self.index_manager.get_or_create(&memory.collection, dims)?;
            if let Err(e) = index.add(seq, &memory.embedding) {
                // Rollback vectors added so far
                for (m, &s) in memories.iter().zip(seqs.iter()).take(vectors_added) {
                    if let Some(idx) = self.index_manager.get(&m.collection) {
                        let _ = idx.remove(s);
                    }
                }
                // txn drops without commit = automatic rollback
                return Err(e);
            }
            vectors_added += 1;
        }

        // Commit — if fails, rollback all vectors
        match txn.commit() {
            Ok(()) => Ok(memories),
            Err(e) => {
                for (m, &s) in memories.iter().zip(seqs.iter()) {
                    if let Some(idx) = self.index_manager.get(&m.collection) {
                        let _ = idx.remove(s);
                    }
                }
                Err(MastError::from(e))
            }
        }
    }

    pub async fn recall(
        &mut self,
        request: RecallRequest,
        embedder: &dyn Embedder,
    ) -> Result<Vec<RecallResult>, MastError> {
        let limit = if request.limit == 0 {
            self.config.default_limit
        } else {
            request.limit
        };

        let now = unix_now();
        let ctx = ScoreContext {
            now,
            reference_time: request.reference_time.unwrap_or(now),
            tier_weights: request
                .tier_weights
                .unwrap_or(self.config.default_tier_weights),
            time_decay: request.time_decay.or(self.config.default_time_decay),
            compacted_source_weight: self.config.compacted_source_weight,
        };

        let anchor = request.time_anchor.or_else(|| {
            request
                .query
                .as_deref()
                .or(request.text_query.as_deref())
                .and_then(|q| temporal::parse_time_anchor(q, ctx.reference_time))
        });

        // The anchor boost re-ranks, so it needs more candidates than `limit`.
        let pool = if anchor.is_some() {
            (limit * 5).max(ANCHOR_CANDIDATE_POOL)
        } else {
            limit
        };

        let mut recall_results = match &request.search_mode {
            SearchMode::Vector => self.recall_vector(&request, embedder, pool, &ctx).await?,
            SearchMode::FullText => self.recall_fulltext(&request, pool, &ctx)?,
            SearchMode::Hybrid {
                vector_weight,
                text_weight,
            } => {
                self.recall_hybrid(&request, embedder, pool, &ctx, *vector_weight, *text_weight)
                    .await?
            }
        };

        if let Some(anchor) = anchor {
            for result in &mut recall_results {
                result.score *= temporal::anchor_boost(&anchor, result.memory.created_at);
            }
            sort_by_score(&mut recall_results);
        }
        recall_results.truncate(limit);

        // Defer access stats — buffer until threshold
        for result in &recall_results {
            self.pending_access_updates.push((
                request.collection.clone(),
                result.memory.id,
                now,
            ));
        }
        if self.pending_access_updates.len() >= ACCESS_STATS_FLUSH_THRESHOLD {
            self.flush_access_stats()?;
        }

        Ok(recall_results)
    }

    async fn recall_vector(
        &self,
        request: &RecallRequest,
        embedder: &dyn Embedder,
        limit: usize,
        ctx: &ScoreContext,
    ) -> Result<Vec<RecallResult>, MastError> {
        let query_embedding = match &request.query_embedding {
            Some(emb) => emb.clone(),
            None => match &request.query {
                Some(q) => embedder.embed(q).await?,
                None => {
                    return Err(MastError::InvalidInput(
                        "either query or query_embedding must be provided".into(),
                    ))
                }
            },
        };

        if query_embedding.is_empty() {
            return Err(MastError::InvalidInput(
                "query embedding must not be empty".into(),
            ));
        }

        let index = match self.index_manager.get(&request.collection) {
            Some(idx) => idx,
            None => return Ok(Vec::new()),
        };

        if index.dimensions() != query_embedding.len() {
            return Err(MastError::InvalidInput(format!(
                "query dimensions {} do not match collection dimensions {}",
                query_embedding.len(),
                index.dimensions(),
            )));
        }

        let fetch_limit = limit * 3 + 10;

        let results = if let Some(filter) = &request.filter {
            let allowed_ids = MetadataIndex::query(&self.storage, &request.collection, filter)?;
            let mut allowed_seqs = std::collections::HashSet::new();
            for uid in &allowed_ids {
                if let Some(seq) = self.storage.get_seq_for_ulid(&request.collection, uid)? {
                    allowed_seqs.insert(seq);
                }
            }
            index.filtered_search(&query_embedding, fetch_limit, |seq| {
                allowed_seqs.contains(&seq)
            })?
        } else {
            index.search(&query_embedding, fetch_limit)?
        };

        let mut recall_results = Vec::new();
        for (seq, distance) in results {
            if let Some(ulid) = self.storage.get_ulid_for_seq(&request.collection, seq)? {
                if let Some(memory) = self.storage.read_memory(&request.collection, &ulid)? {
                    if let Some(score) = ctx.score(&memory, 1.0 - distance) {
                        recall_results.push(RecallResult { memory, score });
                    }
                }
            }
        }

        sort_by_score(&mut recall_results);
        recall_results.truncate(limit);
        Ok(recall_results)
    }

    fn recall_fulltext(
        &self,
        request: &RecallRequest,
        limit: usize,
        ctx: &ScoreContext,
    ) -> Result<Vec<RecallResult>, MastError> {
        let text_query = request
            .text_query
            .as_deref()
            .or(request.query.as_deref())
            .ok_or_else(|| {
                MastError::InvalidInput(
                    "text_query or query must be provided for fulltext search".into(),
                )
            })?;

        let fetch_limit = limit * 3 + 10;
        let ft_results =
            FullTextIndex::search(&self.storage, &request.collection, text_query, fetch_limit)?;

        // Apply metadata filter if present
        let allowed_ids = if let Some(filter) = &request.filter {
            Some(MetadataIndex::query(
                &self.storage,
                &request.collection,
                filter,
            )?)
        } else {
            None
        };

        let mut recall_results = Vec::new();
        for (ulid, bm25_score) in ft_results {
            if let Some(ref allowed) = allowed_ids {
                if !allowed.contains(&ulid) {
                    continue;
                }
            }
            if let Some(memory) = self.storage.read_memory(&request.collection, &ulid)? {
                if let Some(score) = ctx.score(&memory, bm25_score) {
                    recall_results.push(RecallResult { memory, score });
                }
            }
        }

        sort_by_score(&mut recall_results);
        recall_results.truncate(limit);
        Ok(recall_results)
    }

    /// Weighted sum of the vector and full-text scores. Vector similarity is
    /// already on a 0..1 scale and is used as is. BM25 has no fixed scale, so it
    /// is divided by the best BM25 score in the candidate set.
    async fn recall_hybrid(
        &self,
        request: &RecallRequest,
        embedder: &dyn Embedder,
        limit: usize,
        ctx: &ScoreContext,
        vector_weight: f32,
        text_weight: f32,
    ) -> Result<Vec<RecallResult>, MastError> {
        let vec_results = self
            .recall_vector(request, embedder, limit * 3, ctx)
            .await?;
        let ft_results = self.recall_fulltext(request, limit * 3, ctx)?;

        let best_text = ft_results.iter().map(|r| r.score).fold(0.0_f32, f32::max);

        let mut merged: std::collections::HashMap<Ulid, (Memory, f32)> =
            std::collections::HashMap::new();
        let vector = vec_results
            .into_iter()
            .map(|r| (vector_weight * r.score, r));
        let text = ft_results.into_iter().map(|r| {
            let scaled = if best_text > 0.0 { r.score / best_text } else { 0.0 };
            (text_weight * scaled, r)
        });
        for (contribution, r) in vector.chain(text) {
            merged
                .entry(r.memory.id)
                .and_modify(|(_, s)| *s += contribution)
                .or_insert((r.memory, contribution));
        }

        let mut recall_results: Vec<RecallResult> = merged
            .into_values()
            .map(|(memory, score)| RecallResult { memory, score })
            .collect();

        sort_by_score(&mut recall_results);
        recall_results.truncate(limit);
        Ok(recall_results)
    }

    pub fn get(&self, collection: &str, id: &Ulid) -> Result<Option<Memory>, MastError> {
        self.storage.read_memory(collection, id)
    }

    pub fn delete(&mut self, collection: &str, id: &Ulid) -> Result<(), MastError> {
        let memory = self.storage.read_memory(collection, id)?;
        let content = memory.as_ref().map(|m| m.content.clone());
        let metadata = memory
            .as_ref()
            .map(|m| &m.metadata)
            .cloned()
            .unwrap_or_default();

        let txn = self.storage.begin_write_txn()?;
        let seq = Storage::delete_memory_in_txn(&txn, collection, id, &metadata)?;

        if let Some(ref content) = content {
            Storage::ft_remove_in_txn(&txn, collection, id, content)?;
        }

        if let Some(seq) = seq {
            if let Some(idx) = self.index_manager.get(collection) {
                let _ = idx.remove(seq);
            }
        }

        txn.commit()?;
        Ok(())
    }

    pub fn delete_batch(&mut self, collection: &str, ids: &[Ulid]) -> Result<(), MastError> {
        if ids.is_empty() {
            return Ok(());
        }

        // Read all memories first (read txn) to get content + metadata for cleanup
        let mut to_delete: Vec<(Ulid, String, std::collections::HashMap<String, String>)> =
            Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(memory) = self.storage.read_memory(collection, id)? {
                to_delete.push((memory.id, memory.content, memory.metadata));
            }
        }

        if to_delete.is_empty() {
            return Ok(());
        }

        let txn = self.storage.begin_write_txn()?;
        for (id, content, metadata) in &to_delete {
            let seq = Storage::delete_memory_in_txn(&txn, collection, id, metadata)?;
            Storage::ft_remove_in_txn(&txn, collection, id, content)?;

            if let Some(seq) = seq {
                if let Some(idx) = self.index_manager.get(collection) {
                    let _ = idx.remove(seq);
                }
            }
        }
        txn.commit()?;

        Ok(())
    }

    pub fn list(&self, collection: &str) -> Result<Vec<Memory>, MastError> {
        self.storage.list_memories(collection)
    }

    pub fn info(&self) -> Result<Vec<CollectionInfo>, MastError> {
        self.storage.list_collections()
    }

    pub fn vacuum(&mut self, collection: &str) -> Result<VacuumResult, MastError> {
        let now = unix_now();

        let memories = self.storage.list_memories(collection)?;
        let expired_ids: Vec<Ulid> = memories
            .iter()
            .filter(|m| {
                if let Some(ttl) = m.ttl {
                    now - m.created_at >= ttl
                } else {
                    false
                }
            })
            .map(|m| m.id)
            .collect();

        let expired_count = expired_ids.len();
        self.delete_batch(collection, &expired_ids)?;

        Ok(VacuumResult {
            collection: collection.to_string(),
            expired_count,
        })
    }

    pub async fn compact(
        &mut self,
        request: CompactRequest,
        compactor: &dyn Compactor,
        embedder: &dyn Embedder,
    ) -> Result<CompactResult, MastError> {
        if request.max_batch_size == 0 {
            return Err(MastError::InvalidInput(
                "max_batch_size must be > 0".into(),
            ));
        }

        let now = unix_now();

        let memories = self.storage.list_memories(&request.collection)?;

        // Filter candidates
        let candidates: Vec<&Memory> = memories
            .iter()
            .filter(|m| {
                m.tier == request.source_tier && m.compacted_at.is_none()
            })
            .filter(|m| match &request.strategy {
                CompactionStrategy::Age { older_than_secs } => {
                    now - m.created_at > *older_than_secs
                }
                CompactionStrategy::AccessDecay => m.access_count == 0,
                CompactionStrategy::Hybrid => {
                    // Combine age and access: old AND rarely accessed
                    now - m.created_at > 86400 && m.access_count < 3
                }
                CompactionStrategy::Manual { ids } => ids.contains(&m.id),
            })
            .collect();

        let source_count = candidates.len();

        if request.dry_run {
            let source_memory_ids: Vec<Ulid> = candidates.iter().map(|m| m.id).collect();
            return Ok(CompactResult {
                collection: request.collection,
                source_count,
                compacted_count: 0,
                new_memory_ids: vec![],
                source_memory_ids,
            });
        }

        let mut all_new_ids = Vec::new();
        let mut all_source_ids = Vec::new();

        for chunk in candidates.chunks(request.max_batch_size) {
            let chunk_memories: Vec<Memory> = chunk.iter().map(|m| (*m).clone()).collect();
            let synthesized = compactor.synthesize(&chunk_memories).await?;

            let embedding = embedder.embed(&synthesized.content).await?;

            let new_memory = Memory {
                id: Ulid::new(),
                collection: request.collection.clone(),
                content: synthesized.content,
                embedding,
                metadata: synthesized.metadata,
                tier: request.target_tier,
                created_at: now,
                access_count: 0,
                last_accessed: now,
                ttl: None,
                compacted_at: None,
                parent_ids: chunk.iter().map(|m| m.id).collect(),
            };

            // Single transaction for compact write + metadata + fulltext + mark_compacted
            let txn = self.storage.begin_write_txn()?;
            let seq = Storage::write_memory_in_txn(&txn, &new_memory)?;

            if !new_memory.metadata.is_empty() {
                for (key, value) in &new_memory.metadata {
                    Storage::append_meta_in_txn(
                        &txn,
                        &new_memory.collection,
                        key,
                        value,
                        &new_memory.id,
                    )?;
                }
            }

            Storage::ft_insert_in_txn(
                &txn,
                &new_memory.collection,
                &new_memory.id,
                &new_memory.content,
            )?;

            // Mark source memories as compacted within the same transaction
            for m in chunk {
                let ulid_bytes = m.id.to_bytes();
                let mut table = txn.open_table(storage::tables::MEMORIES)?;
                let data = {
                    table
                        .get((request.collection.as_str(), ulid_bytes.as_slice()))?
                        .map(|v| v.value().to_vec())
                };
                if let Some(data) = data {
                    let mut memory = storage::serde::deserialize_memory(&data)?;
                    memory.compacted_at = Some(now);
                    let encoded = storage::serde::serialize_memory(&memory)?;
                    table.insert(
                        (request.collection.as_str(), ulid_bytes.as_slice()),
                        encoded.as_slice(),
                    )?;
                }
            }

            let dims = new_memory.embedding.len();
            let index = self.index_manager.get_or_create(&new_memory.collection, dims)?;
            if let Err(e) = index.add(seq, &new_memory.embedding) {
                // txn drops = rollback
                return Err(e);
            }

            match txn.commit() {
                Ok(()) => {}
                Err(e) => {
                    let _ = index.remove(seq);
                    return Err(MastError::from(e));
                }
            }

            all_new_ids.push(new_memory.id);
            for m in chunk {
                all_source_ids.push(m.id);
            }
        }

        Ok(CompactResult {
            collection: request.collection,
            source_count,
            compacted_count: all_source_ids.len(),
            new_memory_ids: all_new_ids,
            source_memory_ids: all_source_ids,
        })
    }

    // --- Graph API ---

    /// Add an edge, or update the edge with the same source, target, relation
    /// and `valid_from`. A fact that changed over time is one edge per interval.
    pub fn relate(&self, request: RelateRequest) -> Result<Ulid, MastError> {
        if let (Some(from), Some(to)) = (request.valid_from, request.valid_to) {
            if to <= from {
                return Err(MastError::InvalidInput(format!(
                    "valid_to ({to}) must be after valid_from ({from})"
                )));
            }
        }

        let now = unix_now();

        let existing = self.storage.get_edges_for_entity(
            &request.collection,
            &request.source,
            Some(&request.relation),
            storage::EdgeDirection::Forward,
        )?;

        for edge in existing {
            if edge.target == request.target && edge.valid_from == request.valid_from {
                let updated = Edge {
                    weight: request.weight,
                    memory_ids: request.memory_ids,
                    valid_to: request.valid_to,
                    updated_at: now,
                    ..edge
                };
                self.storage.update_edge(&updated)?;
                return Ok(updated.id);
            }
        }

        let edge = Edge {
            id: Ulid::new(),
            collection: request.collection,
            source: request.source,
            target: request.target,
            relation: request.relation,
            weight: request.weight,
            memory_ids: request.memory_ids,
            created_at: now,
            updated_at: now,
            valid_from: request.valid_from,
            valid_to: request.valid_to,
        };
        let id = edge.id;
        self.storage.write_edge(&edge)?;
        Ok(id)
    }

    /// Mark a fact as no longer true by setting `valid_to` on its open edges.
    /// Returns the number of edges closed.
    pub fn invalidate(&self, request: InvalidateRequest) -> Result<usize, MastError> {
        let ended_at = request.ended_at.unwrap_or_else(unix_now);

        let open: Vec<Edge> = self
            .storage
            .get_edges_for_entity(
                &request.collection,
                &request.source,
                Some(&request.relation),
                storage::EdgeDirection::Forward,
            )?
            .into_iter()
            .filter(|e| e.target == request.target && e.valid_to.is_none())
            .collect();

        if open.is_empty() {
            return Err(MastError::NotFound(format!(
                "no open edge from '{}' to '{}' with relation '{}'",
                request.source, request.target, request.relation
            )));
        }
        if let Some(from) = open.iter().filter_map(|e| e.valid_from).find(|&f| ended_at <= f) {
            return Err(MastError::InvalidInput(format!(
                "ended_at ({ended_at}) must be after valid_from ({from})"
            )));
        }

        let now = unix_now();
        for edge in &open {
            self.storage.update_edge(&Edge {
                valid_to: Some(ended_at),
                updated_at: now,
                ..edge.clone()
            })?;
        }
        Ok(open.len())
    }

    /// Delete every edge from `source` to `target` with `relation`, across all
    /// validity intervals.
    pub fn unrelate(
        &self,
        collection: &str,
        source: &str,
        target: &str,
        relation: &str,
    ) -> Result<(), MastError> {
        let edges = self.storage.get_edges_for_entity(
            collection,
            source,
            Some(relation),
            storage::EdgeDirection::Forward,
        )?;

        let mut removed = 0;
        for edge in edges.iter().filter(|e| e.target == target) {
            self.storage
                .delete_edge(collection, &edge.id, source, target, relation)?;
            removed += 1;
        }

        if removed == 0 {
            return Err(MastError::NotFound(format!(
                "no edge from '{source}' to '{target}' with relation '{relation}'"
            )));
        }
        Ok(())
    }

    pub fn traverse(&self, request: TraverseRequest) -> Result<Vec<Edge>, MastError> {
        let mut visited = HashSet::new();
        let mut result = Vec::new();
        let mut queue: VecDeque<(String, usize)> = VecDeque::new();

        queue.push_back((request.start.clone(), 0));
        visited.insert(request.start.clone());

        while let Some((entity, depth)) = queue.pop_front() {
            if depth >= request.max_depth {
                continue;
            }

            let edges = self.storage.get_edges_for_entity(
                &request.collection,
                &entity,
                request.relation_filter.as_deref(),
                storage::EdgeDirection::Forward,
            )?;

            for edge in edges {
                if let Some(min_w) = request.min_weight {
                    if edge.weight < min_w {
                        continue;
                    }
                }
                if let Some(as_of) = request.as_of {
                    if !edge.is_valid_at(as_of) {
                        continue;
                    }
                }

                let target = edge.target.clone();
                result.push(edge);

                if !visited.contains(&target) {
                    visited.insert(target.clone());
                    queue.push_back((target, depth + 1));
                }
            }
        }

        Ok(result)
    }

    pub fn subgraph(&self, collection: &str) -> Result<Graph, MastError> {
        let edges = self.storage.list_edges(collection)?;
        Ok(Graph {
            collection: collection.to_string(),
            edges,
        })
    }

    pub fn flush_access_stats(&mut self) -> Result<(), MastError> {
        if self.pending_access_updates.is_empty() {
            return Ok(());
        }

        // Group by collection
        let mut by_collection: std::collections::HashMap<String, Vec<(Ulid, i64)>> =
            std::collections::HashMap::new();
        for (collection, id, now) in self.pending_access_updates.drain(..) {
            by_collection
                .entry(collection)
                .or_default()
                .push((id, now));
        }

        for (collection, updates) in &by_collection {
            let ids: Vec<Ulid> = updates.iter().map(|(id, _)| *id).collect();
            let now = updates.last().map(|(_, t)| *t).unwrap_or(0);
            self.storage.update_access_stats(collection, &ids, now)?;
        }

        Ok(())
    }

    pub fn close(&mut self) -> Result<(), MastError> {
        self.flush_access_stats()?;
        self.index_manager.flush_all(&self.storage)
    }
}

/// Candidates fetched before the time anchor boost re-ranks them.
const ANCHOR_CANDIDATE_POOL: usize = 50;

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn sort_by_score(results: &mut [RecallResult]) {
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// Everything besides relevance that goes into a recall score.
struct ScoreContext {
    now: i64,
    reference_time: i64,
    tier_weights: TierWeights,
    time_decay: Option<TimeDecay>,
    compacted_source_weight: f32,
}

impl ScoreContext {
    /// Final score for a memory with the given relevance, or `None` when the
    /// memory is not returned: its TTL expired, or it was compacted and
    /// compacted sources are switched off.
    fn score(&self, memory: &Memory, relevance: f32) -> Option<f32> {
        if let Some(ttl) = memory.ttl {
            if self.now - memory.created_at >= ttl {
                return None;
            }
        }
        let compacted = match memory.compacted_at {
            Some(_) if self.compacted_source_weight <= 0.0 => return None,
            Some(_) => self.compacted_source_weight,
            None => 1.0,
        };
        Some(relevance * self.tier_weights.weight_for(memory.tier) * self.decay(memory.created_at) * compacted)
    }

    /// `0.5^(age / half_life)`, with age measured back from the reference time.
    fn decay(&self, created_at: i64) -> f32 {
        match self.time_decay {
            Some(td) if td.half_life_secs > 0 => {
                let age_secs = (self.reference_time - created_at).max(0) as f64;
                0.5_f64.powf(age_secs / td.half_life_secs as f64) as f32
            }
            _ => 1.0,
        }
    }
}
