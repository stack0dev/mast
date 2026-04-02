use crate::error::MastError;
use crate::storage::Storage;
use rust_stemmers::{Algorithm, Stemmer};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

const STOP_WORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "but", "by", "do", "for", "from", "had", "has",
    "have", "he", "her", "his", "how", "if", "in", "into", "is", "it", "its", "me", "my", "no",
    "not", "of", "on", "or", "our", "own", "she", "so", "than", "that", "the", "their", "them",
    "then", "there", "these", "they", "this", "to", "up", "us", "was", "we", "were", "what",
    "when", "who", "will", "with", "you", "your",
];

thread_local! {
    static STEMMER: Stemmer = Stemmer::create(Algorithm::English);
}

pub fn tokenize(text: &str) -> Vec<String> {
    STEMMER.with(|stemmer| {
        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| s.len() >= 2)
            .filter(|s| !STOP_WORDS.contains(s))
            .map(|s| stemmer.stem(s).into_owned())
            .collect()
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostingEntry {
    pub ulid_bytes: [u8; 16],
    pub term_freq: u32,
    pub field_length: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostingList {
    pub doc_freq: u32,
    pub entries: Vec<PostingEntry>,
}

pub struct FullTextIndex;

impl FullTextIndex {
    pub fn insert(
        storage: &Storage,
        collection: &str,
        id: &Ulid,
        content: &str,
    ) -> Result<(), MastError> {
        let tokens = tokenize(content);
        if tokens.is_empty() {
            return Ok(());
        }

        let field_length = tokens.len() as u32;
        let ulid_bytes = id.to_bytes();

        // Count term frequencies
        let mut term_freqs = std::collections::HashMap::new();
        for token in &tokens {
            *term_freqs.entry(token.clone()).or_insert(0u32) += 1;
        }

        // Update posting lists for each unique term
        for (term, tf) in &term_freqs {
            let mut posting = match storage.read_ft_posting(collection, term)? {
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
            storage.write_ft_posting(collection, term, &encoded)?;
        }

        // Update collection stats
        let (doc_count, total_tokens) = storage.read_ft_stats(collection)?;
        storage.write_ft_stats(collection, doc_count + 1, total_tokens + field_length as u64)?;

        Ok(())
    }

    pub fn remove(
        storage: &Storage,
        collection: &str,
        id: &Ulid,
        content: &str,
    ) -> Result<(), MastError> {
        let tokens = tokenize(content);
        if tokens.is_empty() {
            return Ok(());
        }

        let ulid_bytes = id.to_bytes();

        // Get unique terms
        let unique_terms: std::collections::HashSet<&str> =
            tokens.iter().map(|s| s.as_str()).collect();

        let mut removed_field_length = 0u32;

        for term in unique_terms {
            if let Some(data) = storage.read_ft_posting(collection, term)? {
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
                    storage.delete_ft_posting(collection, term)?;
                } else {
                    let encoded = bincode::serialize(&posting)?;
                    storage.write_ft_posting(collection, term, &encoded)?;
                }
            }
        }

        // Update collection stats
        if removed_field_length > 0 {
            let (doc_count, total_tokens) = storage.read_ft_stats(collection)?;
            storage.write_ft_stats(
                collection,
                doc_count.saturating_sub(1),
                total_tokens.saturating_sub(removed_field_length as u64),
            )?;
        }

        Ok(())
    }

    pub fn search(
        storage: &Storage,
        collection: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<(Ulid, f32)>, MastError> {
        let query_tokens = tokenize(query);
        if query_tokens.is_empty() {
            return Ok(Vec::new());
        }

        let (doc_count, total_tokens) = storage.read_ft_stats(collection)?;
        if doc_count == 0 {
            return Ok(Vec::new());
        }

        let avgdl = total_tokens as f64 / doc_count as f64;
        let n_docs = doc_count as f64;
        let k1: f64 = 1.2;
        let b: f64 = 0.75;

        // Collect unique query terms
        let unique_terms: std::collections::HashSet<&str> =
            query_tokens.iter().map(|s| s.as_str()).collect();

        // Accumulate BM25 scores per document
        let mut scores: std::collections::HashMap<[u8; 16], f64> =
            std::collections::HashMap::new();

        for term in unique_terms {
            let posting = match storage.read_ft_posting(collection, term)? {
                Some(data) => bincode::deserialize::<PostingList>(&data)?,
                None => continue,
            };

            let df = posting.doc_freq as f64;
            let idf = ((n_docs - df + 0.5) / (df + 0.5) + 1.0).ln();

            for entry in &posting.entries {
                let tf = entry.term_freq as f64;
                let dl = entry.field_length as f64;
                let tf_component = (tf * (k1 + 1.0)) / (tf + k1 * (1.0 - b + b * dl / avgdl));
                let score = idf * tf_component;

                *scores.entry(entry.ulid_bytes).or_insert(0.0) += score;
            }
        }

        // Sort by score descending
        let mut results: Vec<([u8; 16], f64)> = scores.into_iter().collect();
        results.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit);

        Ok(results
            .into_iter()
            .map(|(ulid_bytes, score)| (Ulid::from_bytes(ulid_bytes), score as f32))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_basic() {
        let tokens = tokenize("Hello World! This is a test.");
        // "hello" -> "hello", "world" -> "world", "test" -> "test" (stemmed forms)
        assert_eq!(tokens, vec!["hello", "world", "test"]);
    }

    #[test]
    fn tokenize_filters_short_tokens() {
        let tokens = tokenize("I am a ok");
        // "i" and "a" filtered (len < 2), "am" kept (stemmed), "ok" kept
        assert!(tokens.contains(&"am".to_string()));
        assert!(tokens.contains(&"ok".to_string()));
        assert!(!tokens.contains(&"i".to_string()));
        assert!(!tokens.contains(&"a".to_string()));
    }

    #[test]
    fn tokenize_filters_stop_words() {
        let tokens = tokenize("the quick brown fox");
        assert!(!tokens.contains(&"the".to_string()));
        assert!(tokens.contains(&"quick".to_string()));
        assert!(tokens.contains(&"brown".to_string()));
        assert!(tokens.contains(&"fox".to_string()));
    }

    #[test]
    fn tokenize_empty() {
        assert!(tokenize("").is_empty());
        assert!(tokenize("   ").is_empty());
    }

    #[test]
    fn tokenize_numbers() {
        let tokens = tokenize("version 2 or version 10");
        assert!(tokens.contains(&"version".to_string()));
        assert!(tokens.contains(&"10".to_string()));
    }

    #[test]
    fn tokenize_stemming() {
        // "running" stems to "run", "runs" stems to "run"
        let tokens = tokenize("running runs");
        assert_eq!(tokens, vec!["run", "run"]);
    }

    #[test]
    fn stemmed_query_matches_stemmed_doc() {
        let (_dir, storage) = test_storage();
        let id = Ulid::new();
        FullTextIndex::insert(&storage, "test", &id, "the dogs are running quickly").unwrap();

        // "runs" stems to "run", should match "running" (also stems to "run")
        let results = FullTextIndex::search(&storage, "test", "runs", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, id);
    }

    #[test]
    fn posting_list_roundtrip() {
        let pl = PostingList {
            doc_freq: 3,
            entries: vec![PostingEntry {
                ulid_bytes: [1; 16],
                term_freq: 2,
                field_length: 10,
            }],
        };
        let encoded = bincode::serialize(&pl).unwrap();
        let decoded: PostingList = bincode::deserialize(&encoded).unwrap();
        assert_eq!(decoded.doc_freq, 3);
        assert_eq!(decoded.entries.len(), 1);
        assert_eq!(decoded.entries[0].term_freq, 2);
    }

    fn test_storage() -> (tempfile::TempDir, Storage) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.mast");
        let storage = Storage::open(&path).unwrap();
        (dir, storage)
    }

    #[test]
    fn insert_and_search() {
        let (_dir, storage) = test_storage();

        let id1 = Ulid::new();
        let id2 = Ulid::new();

        FullTextIndex::insert(&storage, "test", &id1, "the quick brown fox jumps over the lazy dog")
            .unwrap();
        FullTextIndex::insert(&storage, "test", &id2, "a fast brown car drives north")
            .unwrap();

        let results = FullTextIndex::search(&storage, "test", "quick brown", 10).unwrap();
        assert!(!results.is_empty());
        // id1 should score higher (has both "quick" and "brown")
        assert_eq!(results[0].0, id1);
    }

    #[test]
    fn insert_and_remove() {
        let (_dir, storage) = test_storage();

        let id = Ulid::new();
        let content = "rust programming language";

        FullTextIndex::insert(&storage, "test", &id, content).unwrap();
        let results = FullTextIndex::search(&storage, "test", "rust", 10).unwrap();
        assert_eq!(results.len(), 1);

        FullTextIndex::remove(&storage, "test", &id, content).unwrap();
        let results = FullTextIndex::search(&storage, "test", "rust", 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn search_empty_collection() {
        let (_dir, storage) = test_storage();
        let results = FullTextIndex::search(&storage, "test", "anything", 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn search_no_matching_terms() {
        let (_dir, storage) = test_storage();
        let id = Ulid::new();
        FullTextIndex::insert(&storage, "test", &id, "rust programming").unwrap();
        let results = FullTextIndex::search(&storage, "test", "python javascript", 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn search_respects_limit() {
        let (_dir, storage) = test_storage();

        for i in 0..5 {
            let id = Ulid::new();
            FullTextIndex::insert(&storage, "test", &id, &format!("document about rust {i}"))
                .unwrap();
        }

        let results = FullTextIndex::search(&storage, "test", "rust", 3).unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn bm25_scoring_term_frequency_matters() {
        let (_dir, storage) = test_storage();

        let id1 = Ulid::new();
        let id2 = Ulid::new();

        FullTextIndex::insert(&storage, "test", &id1, "rust").unwrap();
        FullTextIndex::insert(&storage, "test", &id2, "rust rust rust programming").unwrap();

        let results = FullTextIndex::search(&storage, "test", "rust", 10).unwrap();
        assert_eq!(results.len(), 2);
        // id2 has more occurrences of "rust" so should score higher
        assert_eq!(results[0].0, id2);
    }

    #[test]
    fn collection_isolation() {
        let (_dir, storage) = test_storage();

        let id1 = Ulid::new();
        let id2 = Ulid::new();

        FullTextIndex::insert(&storage, "col_a", &id1, "rust programming").unwrap();
        FullTextIndex::insert(&storage, "col_b", &id2, "python programming").unwrap();

        let results_a = FullTextIndex::search(&storage, "col_a", "rust", 10).unwrap();
        assert_eq!(results_a.len(), 1);
        assert_eq!(results_a[0].0, id1);

        let results_b = FullTextIndex::search(&storage, "col_b", "rust", 10).unwrap();
        assert!(results_b.is_empty());
    }

    #[test]
    fn stats_updated_correctly() {
        let (_dir, storage) = test_storage();

        let id1 = Ulid::new();
        let id2 = Ulid::new();

        FullTextIndex::insert(&storage, "test", &id1, "hello world").unwrap();
        let (doc_count, total_tokens) = storage.read_ft_stats("test").unwrap();
        assert_eq!(doc_count, 1);
        assert_eq!(total_tokens, 2);

        FullTextIndex::insert(&storage, "test", &id2, "goodbye cruel world").unwrap();
        let (doc_count, total_tokens) = storage.read_ft_stats("test").unwrap();
        assert_eq!(doc_count, 2);
        assert_eq!(total_tokens, 5); // 2 + 3

        FullTextIndex::remove(&storage, "test", &id1, "hello world").unwrap();
        let (doc_count, total_tokens) = storage.read_ft_stats("test").unwrap();
        assert_eq!(doc_count, 1);
        assert_eq!(total_tokens, 3);
    }
}
