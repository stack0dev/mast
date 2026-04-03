pub mod file_lock;
pub mod serde;
pub mod tables;

use crate::config::Durability;
use crate::error::MastError;
use crate::index::fulltext::{tokenize, PostingEntry, PostingList};
use crate::types::{CollectionInfo, Edge, Memory};
use file_lock::FileLock;
use redb::{Database, ReadableTable, WriteTransaction};
use std::collections::HashMap;
use std::path::Path;
use tables::*;
use ulid::Ulid;

pub struct Storage {
    pub(crate) db: Database,
    durability: Durability,
    _file_lock: FileLock,
}

impl Storage {
    pub fn open(path: &Path) -> Result<Self, MastError> {
        Self::open_with_durability(path, Durability::Immediate)
    }

    pub fn open_with_durability(
        path: &Path,
        durability: Durability,
    ) -> Result<Self, MastError> {
        // Acquire our own advisory lock first — this handles stale lock detection
        // and PID-based recovery that redb's built-in flock can't do on its own.
        let file_lock = FileLock::acquire(path)?;

        let db = Database::create(path)?;

        let txn = db.begin_write()?;
        {
            let _ = txn.open_table(MEMORIES)?;
            let _ = txn.open_table(COLLECTIONS)?;
            let _ = txn.open_table(SEQ)?;
            let _ = txn.open_table(ULID_TO_SEQ)?;
            let _ = txn.open_table(SEQ_TO_ULID)?;
            let _ = txn.open_table(META_INDEX)?;
            let _ = txn.open_table(VECTOR_INDEX)?;
            let _ = txn.open_table(EDGES)?;
            let _ = txn.open_table(EDGE_INDEX_FWD)?;
            let _ = txn.open_table(EDGE_INDEX_REV)?;
            let _ = txn.open_table(FT_POSTING)?;
            let _ = txn.open_table(FT_STATS)?;
        }
        txn.commit()?;

        Ok(Self {
            db,
            durability,
            _file_lock: file_lock,
        })
    }

    pub fn begin_write_txn(&self) -> Result<WriteTransaction, MastError> {
        let mut txn = self.db.begin_write()?;
        match self.durability {
            Durability::Immediate => {
                txn.set_durability(redb::Durability::Immediate);
            }
            Durability::Eventual => {
                txn.set_durability(redb::Durability::Eventual);
            }
        }
        Ok(txn)
    }

    /// Write a memory within an existing transaction. Does NOT commit.
    pub fn write_memory_in_txn(
        txn: &WriteTransaction,
        memory: &Memory,
    ) -> Result<u64, MastError> {
        let ulid_bytes = memory.id.to_bytes();
        let encoded = serde::serialize_memory(memory)?;

        {
            let mut table = txn.open_table(MEMORIES)?;
            table.insert(
                (memory.collection.as_str(), ulid_bytes.as_slice()),
                encoded.as_slice(),
            )?;
        }

        let seq = {
            let mut seq_table = txn.open_table(SEQ)?;
            let current = {
                seq_table
                    .get(memory.collection.as_str())?
                    .map(|v| v.value())
                    .unwrap_or(0)
            };
            let next = current + 1;
            seq_table.insert(memory.collection.as_str(), next)?;
            current
        };

        {
            let mut u2s = txn.open_table(ULID_TO_SEQ)?;
            u2s.insert(
                (memory.collection.as_str(), ulid_bytes.as_slice()),
                seq,
            )?;
        }
        {
            let seq_be = seq.to_be_bytes();
            let mut s2u = txn.open_table(SEQ_TO_ULID)?;
            s2u.insert(
                (memory.collection.as_str(), seq_be.as_slice()),
                ulid_bytes.as_slice(),
            )?;
        }

        {
            let mut col_table = txn.open_table(COLLECTIONS)?;
            let existing_info: Option<CollectionInfo> = {
                col_table
                    .get(memory.collection.as_str())?
                    .map(|existing| bincode::deserialize(existing.value()))
                    .transpose()?
            };
            let info = match existing_info {
                Some(mut info) => {
                    info.memory_count += 1;
                    info
                }
                None => CollectionInfo {
                    name: memory.collection.clone(),
                    dimensions: memory.embedding.len(),
                    memory_count: 1,
                    created_at: memory.created_at,
                },
            };
            let encoded_info = bincode::serialize(&info)?;
            col_table.insert(memory.collection.as_str(), encoded_info.as_slice())?;
        }

        Ok(seq)
    }

    /// Append a ULID to a metadata index entry within an existing transaction.
    pub fn append_meta_in_txn(
        txn: &WriteTransaction,
        collection: &str,
        key: &str,
        value: &str,
        id: &Ulid,
    ) -> Result<(), MastError> {
        let ulid_bytes = id.to_bytes();
        let mut table = txn.open_table(META_INDEX)?;
        let mut ids: Vec<[u8; 16]> = {
            table
                .get((collection, key, value))?
                .map(|existing| bincode::deserialize(existing.value()))
                .transpose()?
                .unwrap_or_default()
        };
        ids.push(ulid_bytes);
        let encoded = bincode::serialize(&ids)?;
        table.insert((collection, key, value), encoded.as_slice())?;
        Ok(())
    }

    pub fn write_memory(&self, memory: &Memory) -> Result<u64, MastError> {
        let txn = self.db.begin_write()?;
        let seq = Self::write_memory_in_txn(&txn, memory)?;
        txn.commit()?;
        Ok(seq)
    }

    pub fn write_memories_batch(&self, memories: &[Memory]) -> Result<Vec<u64>, MastError> {
        if memories.is_empty() {
            return Ok(vec![]);
        }

        let txn = self.db.begin_write()?;
        let mut seqs = Vec::with_capacity(memories.len());

        {
            let mut mem_table = txn.open_table(MEMORIES)?;
            let mut seq_table = txn.open_table(SEQ)?;
            let mut u2s = txn.open_table(ULID_TO_SEQ)?;
            let mut s2u = txn.open_table(SEQ_TO_ULID)?;
            let mut col_table = txn.open_table(COLLECTIONS)?;

            for memory in memories {
                let ulid_bytes = memory.id.to_bytes();
                let encoded = serde::serialize_memory(memory)?;

                mem_table.insert(
                    (memory.collection.as_str(), ulid_bytes.as_slice()),
                    encoded.as_slice(),
                )?;

                let current = {
                    seq_table
                        .get(memory.collection.as_str())?
                        .map(|v| v.value())
                        .unwrap_or(0)
                };
                let next = current + 1;
                seq_table.insert(memory.collection.as_str(), next)?;
                let seq = current;

                u2s.insert(
                    (memory.collection.as_str(), ulid_bytes.as_slice()),
                    seq,
                )?;

                let seq_be = seq.to_be_bytes();
                s2u.insert(
                    (memory.collection.as_str(), seq_be.as_slice()),
                    ulid_bytes.as_slice(),
                )?;

                let existing_info: Option<CollectionInfo> = {
                    col_table
                        .get(memory.collection.as_str())?
                        .map(|existing| bincode::deserialize(existing.value()))
                        .transpose()?
                };
                let info = match existing_info {
                    Some(mut info) => {
                        info.memory_count += 1;
                        info
                    }
                    None => CollectionInfo {
                        name: memory.collection.clone(),
                        dimensions: memory.embedding.len(),
                        memory_count: 1,
                        created_at: memory.created_at,
                    },
                };
                let encoded_info = bincode::serialize(&info)?;
                col_table.insert(memory.collection.as_str(), encoded_info.as_slice())?;

                seqs.push(seq);
            }
        }

        txn.commit()?;
        Ok(seqs)
    }

    /// Write a batch of memories within an existing transaction, including metadata
    /// and fulltext indexes. Opens each table once for the entire batch.
    /// Does NOT commit the transaction.
    pub fn write_batch_in_txn(
        txn: &WriteTransaction,
        memories: &[Memory],
    ) -> Result<Vec<u64>, MastError> {
        if memories.is_empty() {
            return Ok(vec![]);
        }

        let mut seqs = Vec::with_capacity(memories.len());

        // --- Core tables: MEMORIES, SEQ, ULID_TO_SEQ, SEQ_TO_ULID, COLLECTIONS ---
        {
            let mut mem_table = txn.open_table(MEMORIES)?;
            let mut seq_table = txn.open_table(SEQ)?;
            let mut u2s = txn.open_table(ULID_TO_SEQ)?;
            let mut s2u = txn.open_table(SEQ_TO_ULID)?;
            let mut col_table = txn.open_table(COLLECTIONS)?;

            // Track per-collection seq counter + count locally: 1 read + 1 write each
            // instead of N reads + N writes per item
            let mut seq_counters: HashMap<String, u64> = HashMap::new();
            let mut col_increments: HashMap<String, u64> = HashMap::new();

            for memory in memories {
                let ulid_bytes = memory.id.to_bytes();
                let encoded = serde::serialize_memory(memory)?;

                mem_table.insert(
                    (memory.collection.as_str(), ulid_bytes.as_slice()),
                    encoded.as_slice(),
                )?;

                let counter = if let Some(c) = seq_counters.get_mut(&memory.collection) {
                    c
                } else {
                    let initial = seq_table
                        .get(memory.collection.as_str())?
                        .map(|v| v.value())
                        .unwrap_or(0);
                    seq_counters.insert(memory.collection.clone(), initial);
                    seq_counters.get_mut(&memory.collection).unwrap()
                };
                let seq = *counter;
                *counter += 1;

                u2s.insert(
                    (memory.collection.as_str(), ulid_bytes.as_slice()),
                    seq,
                )?;

                let seq_be = seq.to_be_bytes();
                s2u.insert(
                    (memory.collection.as_str(), seq_be.as_slice()),
                    ulid_bytes.as_slice(),
                )?;

                *col_increments.entry(memory.collection.clone()).or_insert(0) += 1;
                seqs.push(seq);
            }

            // Write final seq counters: 1 write per collection
            for (collection, final_seq) in &seq_counters {
                seq_table.insert(collection.as_str(), *final_seq)?;
            }

            // Batch-update collection counts: one read-modify-write per collection
            for (collection, count) in &col_increments {
                let first_mem = memories.iter().find(|m| &m.collection == collection).unwrap();
                let existing_info: Option<CollectionInfo> = {
                    col_table
                        .get(collection.as_str())?
                        .map(|existing| bincode::deserialize(existing.value()))
                        .transpose()?
                };
                let info = match existing_info {
                    Some(mut info) => {
                        info.memory_count += count;
                        info
                    }
                    None => CollectionInfo {
                        name: collection.clone(),
                        dimensions: first_mem.embedding.len(),
                        memory_count: *count,
                        created_at: first_mem.created_at,
                    },
                };
                let encoded_info = bincode::serialize(&info)?;
                col_table.insert(collection.as_str(), encoded_info.as_slice())?;
            }
        }

        // --- Metadata index: accumulate all (collection, key, value) -> Vec<ulid_bytes> ---
        {
            let mut meta_table = txn.open_table(META_INDEX)?;

            // Accumulate new IDs per (collection, key, value) triple
            let mut meta_acc: HashMap<(String, String, String), Vec<[u8; 16]>> = HashMap::new();
            for memory in memories {
                let ulid_bytes = memory.id.to_bytes();
                for (key, value) in &memory.metadata {
                    meta_acc
                        .entry((memory.collection.clone(), key.clone(), value.clone()))
                        .or_default()
                        .push(ulid_bytes);
                }
            }

            // Merge with existing entries and bulk-write
            for ((collection, key, value), new_ids) in &meta_acc {
                let mut ids: Vec<[u8; 16]> = {
                    meta_table
                        .get((collection.as_str(), key.as_str(), value.as_str()))?
                        .map(|existing| bincode::deserialize(existing.value()))
                        .transpose()?
                        .unwrap_or_default()
                };
                ids.extend_from_slice(new_ids);
                let encoded = bincode::serialize(&ids)?;
                meta_table.insert(
                    (collection.as_str(), key.as_str(), value.as_str()),
                    encoded.as_slice(),
                )?;
            }
        }

        // --- Fulltext index: accumulate postings per (collection, term) ---
        {
            let mut ft_table = txn.open_table(FT_POSTING)?;
            let mut stats_table = txn.open_table(FT_STATS)?;

            // Accumulate per (collection, term) -> Vec<PostingEntry> and doc_freq increment
            let mut posting_acc: HashMap<(String, String), Vec<PostingEntry>> = HashMap::new();
            // Accumulate per collection -> (doc_count_inc, total_tokens_inc)
            let mut stats_acc: HashMap<String, (u64, u64)> = HashMap::new();

            for memory in memories {
                let tokens = tokenize(&memory.content);
                if tokens.is_empty() {
                    continue;
                }

                let field_length = tokens.len() as u32;
                let ulid_bytes = memory.id.to_bytes();

                let mut term_freqs: HashMap<String, u32> = HashMap::new();
                for token in &tokens {
                    *term_freqs.entry(token.clone()).or_insert(0) += 1;
                }

                for (term, tf) in term_freqs {
                    posting_acc
                        .entry((memory.collection.clone(), term))
                        .or_default()
                        .push(PostingEntry {
                            ulid_bytes,
                            term_freq: tf,
                            field_length,
                        });
                }

                let stats = stats_acc.entry(memory.collection.clone()).or_insert((0, 0));
                stats.0 += 1;
                stats.1 += field_length as u64;
            }

            // Merge postings: one read-modify-write per (collection, term)
            for ((collection, term), new_entries) in &posting_acc {
                let posting_data: Option<Vec<u8>> = ft_table
                    .get((collection.as_str(), term.as_str()))?
                    .map(|v| v.value().to_vec());
                let mut posting = match posting_data {
                    Some(data) => bincode::deserialize::<PostingList>(&data)?,
                    None => PostingList {
                        doc_freq: 0,
                        entries: Vec::new(),
                    },
                };
                posting.doc_freq += new_entries.len() as u32;
                posting.entries.extend_from_slice(new_entries);
                let encoded = bincode::serialize(&posting)?;
                ft_table.insert((collection.as_str(), term.as_str()), encoded.as_slice())?;
            }

            // Merge stats: one read-modify-write per collection
            for (collection, (doc_inc, token_inc)) in &stats_acc {
                let (doc_count, total_tokens): (u64, u64) = {
                    stats_table
                        .get(collection.as_str())?
                        .map(|data| bincode::deserialize(data.value()))
                        .transpose()?
                        .unwrap_or((0, 0))
                };
                let encoded =
                    bincode::serialize(&(doc_count + doc_inc, total_tokens + token_inc))?;
                stats_table.insert(collection.as_str(), encoded.as_slice())?;
            }
        }

        Ok(seqs)
    }

    pub fn read_memory(&self, collection: &str, id: &Ulid) -> Result<Option<Memory>, MastError> {
        let ulid_bytes = id.to_bytes();
        let txn = self.db.begin_read()?;
        let table = txn.open_table(MEMORIES)?;

        match table.get((collection, ulid_bytes.as_slice()))? {
            Some(data) => {
                let memory = serde::deserialize_memory(data.value())?;
                Ok(Some(memory))
            }
            None => Ok(None),
        }
    }

    pub fn list_memories(&self, collection: &str) -> Result<Vec<Memory>, MastError> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(MEMORIES)?;

        let mut memories = Vec::new();
        let iter = table.range::<(&str, &[u8])>(..)?;
        for item in iter {
            let (key, value) = item?;
            let (col, _ulid) = key.value();
            if col == collection {
                let memory = serde::deserialize_memory(value.value())?;
                memories.push(memory);
            }
        }

        Ok(memories)
    }

    /// Delete a memory within an existing transaction. Does NOT commit.
    pub fn delete_memory_in_txn(
        txn: &WriteTransaction,
        collection: &str,
        id: &Ulid,
        metadata: &std::collections::HashMap<String, String>,
    ) -> Result<Option<u64>, MastError> {
        let ulid_bytes = id.to_bytes();

        {
            let mut table = txn.open_table(MEMORIES)?;
            table.remove((collection, ulid_bytes.as_slice()))?;
        }

        let seq = {
            let mut u2s = txn.open_table(ULID_TO_SEQ)?;
            let seq = u2s
                .remove((collection, ulid_bytes.as_slice()))?
                .map(|v| v.value());
            seq
        };

        if let Some(seq) = seq {
            let seq_be = seq.to_be_bytes();
            let mut s2u = txn.open_table(SEQ_TO_ULID)?;
            s2u.remove((collection, seq_be.as_slice()))?;
        }

        {
            let mut meta_table = txn.open_table(META_INDEX)?;
            for (key, value) in metadata {
                let existing_ids: Option<Vec<[u8; 16]>> = {
                    meta_table
                        .get((collection, key.as_str(), value.as_str()))?
                        .map(|existing| bincode::deserialize(existing.value()))
                        .transpose()?
                };
                if let Some(mut ids) = existing_ids {
                    ids.retain(|b| b != &ulid_bytes);
                    if ids.is_empty() {
                        meta_table.remove((collection, key.as_str(), value.as_str()))?;
                    } else {
                        let encoded = bincode::serialize(&ids)?;
                        meta_table.insert(
                            (collection, key.as_str(), value.as_str()),
                            encoded.as_slice(),
                        )?;
                    }
                }
            }
        }

        {
            let mut col_table = txn.open_table(COLLECTIONS)?;
            let existing_info: Option<CollectionInfo> = {
                col_table
                    .get(collection)?
                    .map(|existing| bincode::deserialize(existing.value()))
                    .transpose()?
            };
            if let Some(mut info) = existing_info {
                info.memory_count = info.memory_count.saturating_sub(1);
                let encoded = bincode::serialize(&info)?;
                col_table.insert(collection, encoded.as_slice())?;
            }
        }

        Ok(seq)
    }

    pub fn delete_memory(
        &self,
        collection: &str,
        id: &Ulid,
        metadata: &std::collections::HashMap<String, String>,
    ) -> Result<Option<u64>, MastError> {
        let txn = self.db.begin_write()?;
        let seq = Self::delete_memory_in_txn(&txn, collection, id, metadata)?;
        txn.commit()?;
        Ok(seq)
    }

    pub fn update_access_stats(
        &self,
        collection: &str,
        ids: &[Ulid],
        now: i64,
    ) -> Result<(), MastError> {
        if ids.is_empty() {
            return Ok(());
        }

        let txn = self.db.begin_write()?;
        {
            let mut table = txn.open_table(MEMORIES)?;
            for id in ids {
                let ulid_bytes = id.to_bytes();
                let data = {
                    table
                        .get((collection, ulid_bytes.as_slice()))?
                        .map(|v| v.value().to_vec())
                };
                if let Some(data) = data {
                    let mut memory = serde::deserialize_memory(&data)?;
                    memory.access_count += 1;
                    memory.last_accessed = now;
                    let encoded = serde::serialize_memory(&memory)?;
                    table.insert(
                        (collection, ulid_bytes.as_slice()),
                        encoded.as_slice(),
                    )?;
                }
            }
        }
        txn.commit()?;
        Ok(())
    }

    pub fn mark_compacted(
        &self,
        collection: &str,
        id: &Ulid,
        timestamp: i64,
    ) -> Result<(), MastError> {
        let ulid_bytes = id.to_bytes();
        let txn = self.db.begin_write()?;
        {
            let mut table = txn.open_table(MEMORIES)?;
            let data = {
                table
                    .get((collection, ulid_bytes.as_slice()))?
                    .map(|v| v.value().to_vec())
            };
            match data {
                Some(data) => {
                    let mut memory = serde::deserialize_memory(&data)?;
                    memory.compacted_at = Some(timestamp);
                    let encoded = serde::serialize_memory(&memory)?;
                    table.insert(
                        (collection, ulid_bytes.as_slice()),
                        encoded.as_slice(),
                    )?;
                }
                None => {
                    return Err(MastError::NotFound(format!(
                        "memory {id} not found in collection {collection}"
                    )));
                }
            }
        }
        txn.commit()?;
        Ok(())
    }

    pub fn get_seq_for_ulid(&self, collection: &str, id: &Ulid) -> Result<Option<u64>, MastError> {
        let ulid_bytes = id.to_bytes();
        let txn = self.db.begin_read()?;
        let table = txn.open_table(ULID_TO_SEQ)?;

        Ok(table
            .get((collection, ulid_bytes.as_slice()))?
            .map(|v| v.value()))
    }

    pub fn get_ulid_for_seq(&self, collection: &str, seq: u64) -> Result<Option<Ulid>, MastError> {
        let seq_be = seq.to_be_bytes();
        let txn = self.db.begin_read()?;
        let table = txn.open_table(SEQ_TO_ULID)?;

        match table.get((collection, seq_be.as_slice()))? {
            Some(data) => {
                let bytes: [u8; 16] = data.value().try_into().map_err(|_| {
                    MastError::Storage("invalid ULID bytes in seq_to_ulid table".into())
                })?;
                Ok(Some(Ulid::from_bytes(bytes)))
            }
            None => Ok(None),
        }
    }

    pub fn list_collections(&self) -> Result<Vec<CollectionInfo>, MastError> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(COLLECTIONS)?;

        let mut collections = Vec::new();
        let iter = table.range::<&str>(..)?;
        for item in iter {
            let (_key, value) = item?;
            let info: CollectionInfo = bincode::deserialize(value.value())?;
            collections.push(info);
        }

        Ok(collections)
    }

    pub fn write_vector_index(&self, collection: &str, buffer: &[u8]) -> Result<(), MastError> {
        let txn = self.db.begin_write()?;
        {
            let mut table = txn.open_table(VECTOR_INDEX)?;
            table.insert(collection, buffer)?;
        }
        txn.commit()?;
        Ok(())
    }

    pub fn read_vector_index(&self, collection: &str) -> Result<Option<Vec<u8>>, MastError> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(VECTOR_INDEX)?;

        Ok(table.get(collection)?.map(|v| v.value().to_vec()))
    }

    pub fn write_meta_index_entry(
        &self,
        collection: &str,
        key: &str,
        value: &str,
        ids: &[Ulid],
    ) -> Result<(), MastError> {
        let id_bytes: Vec<[u8; 16]> = ids.iter().map(|id| id.to_bytes()).collect();
        let encoded = bincode::serialize(&id_bytes)?;

        let txn = self.db.begin_write()?;
        {
            let mut table = txn.open_table(META_INDEX)?;
            table.insert((collection, key, value), encoded.as_slice())?;
        }
        txn.commit()?;
        Ok(())
    }

    pub fn read_meta_index_entry(
        &self,
        collection: &str,
        key: &str,
        value: &str,
    ) -> Result<Vec<Ulid>, MastError> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(META_INDEX)?;

        match table.get((collection, key, value))? {
            Some(data) => {
                let id_bytes: Vec<[u8; 16]> = bincode::deserialize(data.value())?;
                Ok(id_bytes.into_iter().map(Ulid::from_bytes).collect())
            }
            None => Ok(Vec::new()),
        }
    }

    pub fn append_meta_index_entry(
        &self,
        collection: &str,
        key: &str,
        value: &str,
        id: &Ulid,
    ) -> Result<(), MastError> {
        let txn = self.db.begin_write()?;
        Self::append_meta_in_txn(&txn, collection, key, value, id)?;
        txn.commit()?;
        Ok(())
    }

    // --- Full-text index ---

    pub fn write_ft_posting(
        &self,
        collection: &str,
        term: &str,
        data: &[u8],
    ) -> Result<(), MastError> {
        let txn = self.db.begin_write()?;
        {
            let mut table = txn.open_table(FT_POSTING)?;
            table.insert((collection, term), data)?;
        }
        txn.commit()?;
        Ok(())
    }

    pub fn read_ft_posting(
        &self,
        collection: &str,
        term: &str,
    ) -> Result<Option<Vec<u8>>, MastError> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(FT_POSTING)?;
        Ok(table.get((collection, term))?.map(|v| v.value().to_vec()))
    }

    pub fn delete_ft_posting(
        &self,
        collection: &str,
        term: &str,
    ) -> Result<(), MastError> {
        let txn = self.db.begin_write()?;
        {
            let mut table = txn.open_table(FT_POSTING)?;
            table.remove((collection, term))?;
        }
        txn.commit()?;
        Ok(())
    }

    pub fn write_ft_stats(
        &self,
        collection: &str,
        doc_count: u64,
        total_tokens: u64,
    ) -> Result<(), MastError> {
        let encoded = bincode::serialize(&(doc_count, total_tokens))?;
        let txn = self.db.begin_write()?;
        {
            let mut table = txn.open_table(FT_STATS)?;
            table.insert(collection, encoded.as_slice())?;
        }
        txn.commit()?;
        Ok(())
    }

    pub fn read_ft_stats(
        &self,
        collection: &str,
    ) -> Result<(u64, u64), MastError> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(FT_STATS)?;
        match table.get(collection)? {
            Some(data) => {
                let (doc_count, total_tokens): (u64, u64) = bincode::deserialize(data.value())?;
                Ok((doc_count, total_tokens))
            }
            None => Ok((0, 0)),
        }
    }

    /// Insert all fulltext postings for a document within an existing transaction.
    pub fn ft_insert_in_txn(
        txn: &WriteTransaction,
        collection: &str,
        id: &Ulid,
        content: &str,
    ) -> Result<(), MastError> {
        use crate::index::fulltext::{tokenize, PostingEntry, PostingList};

        let tokens = tokenize(content);
        if tokens.is_empty() {
            return Ok(());
        }

        let field_length = tokens.len() as u32;
        let ulid_bytes = id.to_bytes();

        let mut term_freqs = std::collections::HashMap::new();
        for token in &tokens {
            *term_freqs.entry(token.clone()).or_insert(0u32) += 1;
        }

        {
            let mut ft_table = txn.open_table(FT_POSTING)?;
            for (term, tf) in &term_freqs {
                let posting_data: Option<Vec<u8>> = ft_table
                    .get((collection, term.as_str()))?
                    .map(|v| v.value().to_vec());
                let mut posting = match posting_data {
                    Some(data) => bincode::deserialize::<PostingList>(&data)?,
                    None => PostingList {
                        doc_freq: 0,
                        entries: Vec::new(),
                    },
                };
                posting.doc_freq += 1;
                posting.entries.push(PostingEntry {
                    ulid_bytes,
                    term_freq: *tf,
                    field_length,
                });
                let encoded = bincode::serialize(&posting)?;
                ft_table.insert((collection, term.as_str()), encoded.as_slice())?;
            }
        }

        {
            let mut stats_table = txn.open_table(FT_STATS)?;
            let (doc_count, total_tokens): (u64, u64) = {
                stats_table
                    .get(collection)?
                    .map(|data| bincode::deserialize(data.value()))
                    .transpose()?
                    .unwrap_or((0, 0))
            };
            let encoded =
                bincode::serialize(&(doc_count + 1, total_tokens + field_length as u64))?;
            stats_table.insert(collection, encoded.as_slice())?;
        }

        Ok(())
    }

    /// Remove all fulltext postings for a document within an existing transaction.
    pub fn ft_remove_in_txn(
        txn: &WriteTransaction,
        collection: &str,
        id: &Ulid,
        content: &str,
    ) -> Result<(), MastError> {
        use crate::index::fulltext::{tokenize, PostingList};

        let tokens = tokenize(content);
        if tokens.is_empty() {
            return Ok(());
        }

        let ulid_bytes = id.to_bytes();
        let unique_terms: std::collections::HashSet<&str> =
            tokens.iter().map(|s| s.as_str()).collect();

        let mut removed_field_length = 0u32;

        {
            let mut ft_table = txn.open_table(FT_POSTING)?;
            for term in &unique_terms {
                let posting_data: Option<Vec<u8>> = ft_table
                    .get((collection, *term))?
                    .map(|v| v.value().to_vec());
                if let Some(data) = posting_data {
                    let mut posting: PostingList = bincode::deserialize(&data)?;
                    let before_len = posting.entries.len();
                    posting.entries.retain(|e| {
                        if e.ulid_bytes == ulid_bytes {
                            removed_field_length = e.field_length;
                            false
                        } else {
                            true
                        }
                    });

                    if posting.entries.len() < before_len {
                        posting.doc_freq = posting.doc_freq.saturating_sub(1);
                    }

                    if posting.entries.is_empty() {
                        ft_table.remove((collection, *term))?;
                    } else {
                        let encoded = bincode::serialize(&posting)?;
                        ft_table.insert((collection, *term), encoded.as_slice())?;
                    }
                }
            }
        }

        if removed_field_length > 0 {
            let mut stats_table = txn.open_table(FT_STATS)?;
            let (doc_count, total_tokens): (u64, u64) = {
                stats_table
                    .get(collection)?
                    .map(|data| bincode::deserialize(data.value()))
                    .transpose()?
                    .unwrap_or((0, 0))
            };
            let encoded = bincode::serialize(&(
                doc_count.saturating_sub(1),
                total_tokens.saturating_sub(removed_field_length as u64),
            ))?;
            stats_table.insert(collection, encoded.as_slice())?;
        }

        Ok(())
    }

    // --- Edge CRUD ---

    /// Write an edge within an existing transaction. Does NOT commit.
    pub fn write_edge_in_txn(txn: &WriteTransaction, edge: &Edge) -> Result<(), MastError> {
        let edge_id_bytes = edge.id.to_bytes();
        let encoded = bincode::serialize(edge)?;

        {
            let mut edges = txn.open_table(EDGES)?;
            edges.insert(
                (edge.collection.as_str(), edge_id_bytes.as_slice()),
                encoded.as_slice(),
            )?;

            let mut fwd = txn.open_table(EDGE_INDEX_FWD)?;
            let mut fwd_ids: Vec<[u8; 16]> = {
                fwd.get((edge.collection.as_str(), edge.source.as_str(), edge.relation.as_str()))?
                    .map(|v| bincode::deserialize(v.value()))
                    .transpose()?
                    .unwrap_or_default()
            };
            fwd_ids.push(edge_id_bytes);
            let fwd_encoded = bincode::serialize(&fwd_ids)?;
            fwd.insert(
                (edge.collection.as_str(), edge.source.as_str(), edge.relation.as_str()),
                fwd_encoded.as_slice(),
            )?;

            let mut rev = txn.open_table(EDGE_INDEX_REV)?;
            let mut rev_ids: Vec<[u8; 16]> = {
                rev.get((edge.collection.as_str(), edge.target.as_str(), edge.relation.as_str()))?
                    .map(|v| bincode::deserialize(v.value()))
                    .transpose()?
                    .unwrap_or_default()
            };
            rev_ids.push(edge_id_bytes);
            let rev_encoded = bincode::serialize(&rev_ids)?;
            rev.insert(
                (edge.collection.as_str(), edge.target.as_str(), edge.relation.as_str()),
                rev_encoded.as_slice(),
            )?;
        }

        Ok(())
    }

    pub fn write_edge(&self, edge: &Edge) -> Result<(), MastError> {
        let edge_id_bytes = edge.id.to_bytes();
        let encoded = bincode::serialize(edge)?;

        let txn = self.db.begin_write()?;
        {
            let mut edges = txn.open_table(EDGES)?;
            edges.insert(
                (edge.collection.as_str(), edge_id_bytes.as_slice()),
                encoded.as_slice(),
            )?;

            // Forward index: source -> edge_ids
            let mut fwd = txn.open_table(EDGE_INDEX_FWD)?;
            let mut fwd_ids: Vec<[u8; 16]> = {
                fwd.get((edge.collection.as_str(), edge.source.as_str(), edge.relation.as_str()))?
                    .map(|v| bincode::deserialize(v.value()))
                    .transpose()?
                    .unwrap_or_default()
            };
            fwd_ids.push(edge_id_bytes);
            let fwd_encoded = bincode::serialize(&fwd_ids)?;
            fwd.insert(
                (edge.collection.as_str(), edge.source.as_str(), edge.relation.as_str()),
                fwd_encoded.as_slice(),
            )?;

            // Reverse index: target -> edge_ids
            let mut rev = txn.open_table(EDGE_INDEX_REV)?;
            let mut rev_ids: Vec<[u8; 16]> = {
                rev.get((edge.collection.as_str(), edge.target.as_str(), edge.relation.as_str()))?
                    .map(|v| bincode::deserialize(v.value()))
                    .transpose()?
                    .unwrap_or_default()
            };
            rev_ids.push(edge_id_bytes);
            let rev_encoded = bincode::serialize(&rev_ids)?;
            rev.insert(
                (edge.collection.as_str(), edge.target.as_str(), edge.relation.as_str()),
                rev_encoded.as_slice(),
            )?;
        }
        txn.commit()?;
        Ok(())
    }

    pub fn update_edge(&self, edge: &Edge) -> Result<(), MastError> {
        let edge_id_bytes = edge.id.to_bytes();
        let encoded = bincode::serialize(edge)?;

        let txn = self.db.begin_write()?;
        {
            let mut edges = txn.open_table(EDGES)?;
            edges.insert(
                (edge.collection.as_str(), edge_id_bytes.as_slice()),
                encoded.as_slice(),
            )?;
        }
        txn.commit()?;
        Ok(())
    }

    pub fn read_edge(&self, collection: &str, id: &Ulid) -> Result<Option<Edge>, MastError> {
        let edge_id_bytes = id.to_bytes();
        let txn = self.db.begin_read()?;
        let table = txn.open_table(EDGES)?;

        match table.get((collection, edge_id_bytes.as_slice()))? {
            Some(data) => {
                let edge: Edge = bincode::deserialize(data.value())?;
                Ok(Some(edge))
            }
            None => Ok(None),
        }
    }

    pub fn delete_edge(
        &self,
        collection: &str,
        id: &Ulid,
        source: &str,
        target: &str,
        relation: &str,
    ) -> Result<(), MastError> {
        let edge_id_bytes = id.to_bytes();

        let txn = self.db.begin_write()?;
        {
            let mut edges = txn.open_table(EDGES)?;
            edges.remove((collection, edge_id_bytes.as_slice()))?;

            // Clean forward index
            let mut fwd = txn.open_table(EDGE_INDEX_FWD)?;
            let fwd_ids: Option<Vec<[u8; 16]>> = {
                fwd.get((collection, source, relation))?
                    .map(|v| bincode::deserialize(v.value()))
                    .transpose()?
            };
            if let Some(mut ids) = fwd_ids {
                ids.retain(|b| b != &edge_id_bytes);
                if ids.is_empty() {
                    fwd.remove((collection, source, relation))?;
                } else {
                    let encoded = bincode::serialize(&ids)?;
                    fwd.insert((collection, source, relation), encoded.as_slice())?;
                }
            }

            // Clean reverse index
            let mut rev = txn.open_table(EDGE_INDEX_REV)?;
            let rev_ids: Option<Vec<[u8; 16]>> = {
                rev.get((collection, target, relation))?
                    .map(|v| bincode::deserialize(v.value()))
                    .transpose()?
            };
            if let Some(mut ids) = rev_ids {
                ids.retain(|b| b != &edge_id_bytes);
                if ids.is_empty() {
                    rev.remove((collection, target, relation))?;
                } else {
                    let encoded = bincode::serialize(&ids)?;
                    rev.insert((collection, target, relation), encoded.as_slice())?;
                }
            }
        }
        txn.commit()?;
        Ok(())
    }

    pub fn get_edges_for_entity(
        &self,
        collection: &str,
        entity: &str,
        relation: Option<&str>,
        direction: EdgeDirection,
    ) -> Result<Vec<Edge>, MastError> {
        let txn = self.db.begin_read()?;
        let edges_table = txn.open_table(EDGES)?;

        let index_table = match direction {
            EdgeDirection::Forward => txn.open_table(EDGE_INDEX_FWD)?,
            EdgeDirection::Reverse => txn.open_table(EDGE_INDEX_REV)?,
        };

        let mut result = Vec::new();

        if let Some(rel) = relation {
            // Look up specific relation
            if let Some(data) = index_table.get((collection, entity, rel))? {
                let edge_ids: Vec<[u8; 16]> = bincode::deserialize(data.value())?;
                for eid in edge_ids {
                    if let Some(edge_data) = edges_table.get((collection, eid.as_slice()))? {
                        let edge: Edge = bincode::deserialize(edge_data.value())?;
                        result.push(edge);
                    }
                }
            }
        } else {
            // Scan all relations for this entity — iterate over full index
            let iter = index_table.range::<(&str, &str, &str)>(..)?;
            for item in iter {
                let (key, value) = item?;
                let (col, ent, _rel) = key.value();
                if col == collection && ent == entity {
                    let edge_ids: Vec<[u8; 16]> = bincode::deserialize(value.value())?;
                    for eid in edge_ids {
                        if let Some(edge_data) = edges_table.get((collection, eid.as_slice()))? {
                            let edge: Edge = bincode::deserialize(edge_data.value())?;
                            result.push(edge);
                        }
                    }
                }
            }
        }

        Ok(result)
    }

    pub fn edge_exists(
        &self,
        collection: &str,
        source: &str,
        target: &str,
        relation: &str,
    ) -> Result<bool, MastError> {
        let txn = self.db.begin_read()?;
        let fwd = txn.open_table(EDGE_INDEX_FWD)?;
        let edges_table = txn.open_table(EDGES)?;

        let edge_ids: Vec<[u8; 16]> = match fwd.get((collection, source, relation))? {
            Some(data) => bincode::deserialize(data.value())?,
            None => return Ok(false),
        };

        for eid in edge_ids {
            if let Some(edge_data) = edges_table.get((collection, eid.as_slice()))? {
                let edge: Edge = bincode::deserialize(edge_data.value())?;
                if edge.target == target {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    pub fn list_edges(&self, collection: &str) -> Result<Vec<Edge>, MastError> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(EDGES)?;

        let mut edges = Vec::new();
        let iter = table.range::<(&str, &[u8])>(..)?;
        for item in iter {
            let (key, value) = item?;
            let (col, _eid) = key.value();
            if col == collection {
                let edge: Edge = bincode::deserialize(value.value())?;
                edges.push(edge);
            }
        }

        Ok(edges)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum EdgeDirection {
    Forward,
    Reverse,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Tier;
    use std::collections::HashMap;

    fn test_storage() -> (tempfile::TempDir, Storage) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.mast");
        let storage = Storage::open(&path).unwrap();
        (dir, storage)
    }

    fn make_memory(collection: &str, content: &str) -> Memory {
        Memory {
            id: Ulid::new(),
            collection: collection.into(),
            content: content.into(),
            embedding: vec![0.1, 0.2, 0.3],
            metadata: HashMap::new(),
            tier: Tier::Active,
            created_at: 1000,
            access_count: 0,
            last_accessed: 1000,
            ttl: None,
            compacted_at: None,
            parent_ids: vec![],
        }
    }

    #[test]
    fn open_creates_database() {
        let (_dir, _storage) = test_storage();
    }

    #[test]
    fn write_and_read_memory() {
        let (_dir, storage) = test_storage();
        let mem = make_memory("test", "hello world");
        let id = mem.id;

        storage.write_memory(&mem).unwrap();
        let loaded = storage.read_memory("test", &id).unwrap().unwrap();
        assert_eq!(loaded.id, id);
        assert_eq!(loaded.content, "hello world");
        assert_eq!(loaded.collection, "test");
    }

    #[test]
    fn read_nonexistent_memory_returns_none() {
        let (_dir, storage) = test_storage();
        let result = storage.read_memory("test", &Ulid::new()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn list_memories_returns_all_in_collection() {
        let (_dir, storage) = test_storage();
        let m1 = make_memory("col_a", "first");
        let m2 = make_memory("col_a", "second");
        let m3 = make_memory("col_b", "other");

        storage.write_memory(&m1).unwrap();
        storage.write_memory(&m2).unwrap();
        storage.write_memory(&m3).unwrap();

        let col_a = storage.list_memories("col_a").unwrap();
        assert_eq!(col_a.len(), 2);

        let col_b = storage.list_memories("col_b").unwrap();
        assert_eq!(col_b.len(), 1);
        assert_eq!(col_b[0].content, "other");
    }

    #[test]
    fn list_memories_empty_collection() {
        let (_dir, storage) = test_storage();
        let result = storage.list_memories("nonexistent").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn delete_memory_removes_from_all_tables() {
        let (_dir, storage) = test_storage();
        let mut mem = make_memory("test", "to delete");
        mem.metadata.insert("key".into(), "val".into());
        let id = mem.id;

        storage.write_memory(&mem).unwrap();
        storage.append_meta_index_entry("test", "key", "val", &id).unwrap();

        assert!(storage.read_memory("test", &id).unwrap().is_some());
        assert!(storage.get_seq_for_ulid("test", &id).unwrap().is_some());

        storage.delete_memory("test", &id, &mem.metadata).unwrap();

        assert!(storage.read_memory("test", &id).unwrap().is_none());
        assert!(storage.get_seq_for_ulid("test", &id).unwrap().is_none());

        let meta_ids = storage.read_meta_index_entry("test", "key", "val").unwrap();
        assert!(meta_ids.is_empty());

        let cols = storage.list_collections().unwrap();
        let col = cols.iter().find(|c| c.name == "test").unwrap();
        assert_eq!(col.memory_count, 0);
    }

    #[test]
    fn ulid_seq_bidirectional_mapping() {
        let (_dir, storage) = test_storage();
        let mem = make_memory("test", "mapping test");
        let id = mem.id;

        let seq = storage.write_memory(&mem).unwrap();
        assert_eq!(seq, 0);

        let found_seq = storage.get_seq_for_ulid("test", &id).unwrap().unwrap();
        assert_eq!(found_seq, seq);

        let found_ulid = storage.get_ulid_for_seq("test", seq).unwrap().unwrap();
        assert_eq!(found_ulid, id);
    }

    #[test]
    fn seq_increments_atomically() {
        let (_dir, storage) = test_storage();
        let m1 = make_memory("test", "first");
        let m2 = make_memory("test", "second");
        let m3 = make_memory("test", "third");

        let s1 = storage.write_memory(&m1).unwrap();
        let s2 = storage.write_memory(&m2).unwrap();
        let s3 = storage.write_memory(&m3).unwrap();

        assert_eq!(s1, 0);
        assert_eq!(s2, 1);
        assert_eq!(s3, 2);
    }

    #[test]
    fn seq_per_collection() {
        let (_dir, storage) = test_storage();
        let m_a = make_memory("col_a", "first");
        let m_b = make_memory("col_b", "first");

        let s_a = storage.write_memory(&m_a).unwrap();
        let s_b = storage.write_memory(&m_b).unwrap();

        assert_eq!(s_a, 0);
        assert_eq!(s_b, 0);
    }

    #[test]
    fn nonexistent_seq_returns_none() {
        let (_dir, storage) = test_storage();
        assert!(storage.get_seq_for_ulid("test", &Ulid::new()).unwrap().is_none());
        assert!(storage.get_ulid_for_seq("test", 999).unwrap().is_none());
    }

    #[test]
    fn list_collections() {
        let (_dir, storage) = test_storage();
        let m1 = make_memory("alpha", "a");
        let m2 = make_memory("beta", "b");
        let m3 = make_memory("alpha", "a2");

        storage.write_memory(&m1).unwrap();
        storage.write_memory(&m2).unwrap();
        storage.write_memory(&m3).unwrap();

        let cols = storage.list_collections().unwrap();
        assert_eq!(cols.len(), 2);

        let alpha = cols.iter().find(|c| c.name == "alpha").unwrap();
        assert_eq!(alpha.memory_count, 2);
        assert_eq!(alpha.dimensions, 3);

        let beta = cols.iter().find(|c| c.name == "beta").unwrap();
        assert_eq!(beta.memory_count, 1);
    }

    #[test]
    fn list_collections_empty() {
        let (_dir, storage) = test_storage();
        let cols = storage.list_collections().unwrap();
        assert!(cols.is_empty());
    }

    #[test]
    fn vector_index_write_read_roundtrip() {
        let (_dir, storage) = test_storage();
        let buffer = vec![1u8, 2, 3, 4, 5];

        storage.write_vector_index("test_col", &buffer).unwrap();
        let loaded = storage.read_vector_index("test_col").unwrap().unwrap();
        assert_eq!(loaded, buffer);
    }

    #[test]
    fn vector_index_read_nonexistent() {
        let (_dir, storage) = test_storage();
        assert!(storage.read_vector_index("nope").unwrap().is_none());
    }

    #[test]
    fn meta_index_write_read_roundtrip() {
        let (_dir, storage) = test_storage();
        let ids = vec![Ulid::new(), Ulid::new()];

        storage.write_meta_index_entry("col", "key", "val", &ids).unwrap();
        let loaded = storage.read_meta_index_entry("col", "key", "val").unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0], ids[0]);
        assert_eq!(loaded[1], ids[1]);
    }

    #[test]
    fn meta_index_read_nonexistent() {
        let (_dir, storage) = test_storage();
        let ids = storage.read_meta_index_entry("col", "no", "entry").unwrap();
        assert!(ids.is_empty());
    }

    #[test]
    fn append_meta_index_entry() {
        let (_dir, storage) = test_storage();
        let id1 = Ulid::new();
        let id2 = Ulid::new();

        storage.append_meta_index_entry("col", "k", "v", &id1).unwrap();
        storage.append_meta_index_entry("col", "k", "v", &id2).unwrap();

        let ids = storage.read_meta_index_entry("col", "k", "v").unwrap();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&id1));
        assert!(ids.contains(&id2));
    }

    #[test]
    fn delete_nonexistent_memory_is_ok() {
        let (_dir, storage) = test_storage();
        let result = storage.delete_memory("test", &Ulid::new(), &HashMap::new()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn write_memories_batch() {
        let (_dir, storage) = test_storage();
        let m1 = make_memory("test", "first");
        let m2 = make_memory("test", "second");
        let id1 = m1.id;
        let id2 = m2.id;

        let seqs = storage.write_memories_batch(&[m1, m2]).unwrap();
        assert_eq!(seqs, vec![0, 1]);

        let loaded1 = storage.read_memory("test", &id1).unwrap().unwrap();
        assert_eq!(loaded1.content, "first");
        let loaded2 = storage.read_memory("test", &id2).unwrap().unwrap();
        assert_eq!(loaded2.content, "second");

        let cols = storage.list_collections().unwrap();
        assert_eq!(cols[0].memory_count, 2);
    }

    #[test]
    fn update_access_stats() {
        let (_dir, storage) = test_storage();
        let mem = make_memory("test", "stats");
        let id = mem.id;
        storage.write_memory(&mem).unwrap();

        storage.update_access_stats("test", &[id], 5000).unwrap();

        let loaded = storage.read_memory("test", &id).unwrap().unwrap();
        assert_eq!(loaded.access_count, 1);
        assert_eq!(loaded.last_accessed, 5000);
    }

    #[test]
    fn mark_compacted() {
        let (_dir, storage) = test_storage();
        let mem = make_memory("test", "compact me");
        let id = mem.id;
        storage.write_memory(&mem).unwrap();

        storage.mark_compacted("test", &id, 9999).unwrap();

        let loaded = storage.read_memory("test", &id).unwrap().unwrap();
        assert_eq!(loaded.compacted_at, Some(9999));
    }

    #[test]
    fn edge_write_and_read() {
        let (_dir, storage) = test_storage();
        let edge = Edge {
            id: Ulid::new(),
            collection: "test".into(),
            source: "alice".into(),
            target: "bob".into(),
            relation: "knows".into(),
            weight: 0.9,
            memory_ids: vec![],
            created_at: 1000,
            updated_at: 1000,
        };
        let eid = edge.id;

        storage.write_edge(&edge).unwrap();
        let loaded = storage.read_edge("test", &eid).unwrap().unwrap();
        assert_eq!(loaded.source, "alice");
        assert_eq!(loaded.target, "bob");
    }

    #[test]
    fn edge_forward_index() {
        let (_dir, storage) = test_storage();
        let edge = Edge {
            id: Ulid::new(),
            collection: "test".into(),
            source: "alice".into(),
            target: "bob".into(),
            relation: "knows".into(),
            weight: 0.9,
            memory_ids: vec![],
            created_at: 1000,
            updated_at: 1000,
        };
        storage.write_edge(&edge).unwrap();

        let edges = storage
            .get_edges_for_entity("test", "alice", Some("knows"), EdgeDirection::Forward)
            .unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].target, "bob");
    }

    #[test]
    fn edge_delete() {
        let (_dir, storage) = test_storage();
        let edge = Edge {
            id: Ulid::new(),
            collection: "test".into(),
            source: "alice".into(),
            target: "bob".into(),
            relation: "knows".into(),
            weight: 0.9,
            memory_ids: vec![],
            created_at: 1000,
            updated_at: 1000,
        };
        let eid = edge.id;
        storage.write_edge(&edge).unwrap();

        storage.delete_edge("test", &eid, "alice", "bob", "knows").unwrap();

        assert!(storage.read_edge("test", &eid).unwrap().is_none());
        let fwd = storage
            .get_edges_for_entity("test", "alice", Some("knows"), EdgeDirection::Forward)
            .unwrap();
        assert!(fwd.is_empty());
    }

    #[test]
    fn list_edges() {
        let (_dir, storage) = test_storage();
        let e1 = Edge {
            id: Ulid::new(),
            collection: "test".into(),
            source: "a".into(),
            target: "b".into(),
            relation: "r".into(),
            weight: 1.0,
            memory_ids: vec![],
            created_at: 1000,
            updated_at: 1000,
        };
        let e2 = Edge {
            id: Ulid::new(),
            collection: "test".into(),
            source: "b".into(),
            target: "c".into(),
            relation: "r".into(),
            weight: 1.0,
            memory_ids: vec![],
            created_at: 1000,
            updated_at: 1000,
        };
        storage.write_edge(&e1).unwrap();
        storage.write_edge(&e2).unwrap();

        let edges = storage.list_edges("test").unwrap();
        assert_eq!(edges.len(), 2);
    }
}
