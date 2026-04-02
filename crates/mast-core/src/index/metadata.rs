use crate::error::MastError;
use crate::storage::Storage;
use crate::types::MetadataFilter;
use std::collections::{HashMap, HashSet};
use ulid::Ulid;

pub struct MetadataIndex;

impl MetadataIndex {
    pub fn insert(
        storage: &Storage,
        collection: &str,
        id: &Ulid,
        metadata: &HashMap<String, String>,
    ) -> Result<(), MastError> {
        for (key, value) in metadata {
            storage.append_meta_index_entry(collection, key, value, id)?;
        }
        Ok(())
    }

    pub fn remove(
        storage: &Storage,
        collection: &str,
        id: &Ulid,
        metadata: &HashMap<String, String>,
    ) -> Result<(), MastError> {
        for (key, value) in metadata {
            let mut ids = storage.read_meta_index_entry(collection, key, value)?;
            ids.retain(|existing| existing != id);
            storage.write_meta_index_entry(collection, key, value, &ids)?;
        }
        Ok(())
    }

    pub fn query(
        storage: &Storage,
        collection: &str,
        filter: &MetadataFilter,
    ) -> Result<HashSet<Ulid>, MastError> {
        match filter {
            MetadataFilter::Eq(key, value) => {
                let ids = storage.read_meta_index_entry(collection, key, value)?;
                Ok(ids.into_iter().collect())
            }
            MetadataFilter::In(key, values) => {
                let mut result = HashSet::new();
                for value in values {
                    let ids = storage.read_meta_index_entry(collection, key, value)?;
                    result.extend(ids);
                }
                Ok(result)
            }
            MetadataFilter::And(filters) => {
                let mut iter = filters.iter();
                let first = match iter.next() {
                    Some(f) => Self::query(storage, collection, f)?,
                    None => return Ok(HashSet::new()),
                };
                let mut result = first;
                for f in iter {
                    let set = Self::query(storage, collection, f)?;
                    result = result.intersection(&set).copied().collect();
                }
                Ok(result)
            }
            MetadataFilter::Or(filters) => {
                let mut result = HashSet::new();
                for f in filters {
                    let set = Self::query(storage, collection, f)?;
                    result.extend(set);
                }
                Ok(result)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Storage;

    fn test_storage() -> (tempfile::TempDir, Storage) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.mast");
        let storage = Storage::open(&path).unwrap();
        (dir, storage)
    }

    #[test]
    fn insert_and_query_eq() {
        let (_dir, storage) = test_storage();
        let id1 = Ulid::new();
        let id2 = Ulid::new();

        let meta1 = HashMap::from([("color".into(), "red".into())]);
        let meta2 = HashMap::from([("color".into(), "blue".into())]);

        MetadataIndex::insert(&storage, "col", &id1, &meta1).unwrap();
        MetadataIndex::insert(&storage, "col", &id2, &meta2).unwrap();

        let result = MetadataIndex::query(
            &storage,
            "col",
            &MetadataFilter::Eq("color".into(), "red".into()),
        )
        .unwrap();
        assert_eq!(result.len(), 1);
        assert!(result.contains(&id1));
    }

    #[test]
    fn query_in() {
        let (_dir, storage) = test_storage();
        let id1 = Ulid::new();
        let id2 = Ulid::new();
        let id3 = Ulid::new();

        MetadataIndex::insert(&storage, "col", &id1, &HashMap::from([("size".into(), "s".into())])).unwrap();
        MetadataIndex::insert(&storage, "col", &id2, &HashMap::from([("size".into(), "m".into())])).unwrap();
        MetadataIndex::insert(&storage, "col", &id3, &HashMap::from([("size".into(), "l".into())])).unwrap();

        let result = MetadataIndex::query(
            &storage,
            "col",
            &MetadataFilter::In("size".into(), vec!["s".into(), "l".into()]),
        )
        .unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.contains(&id1));
        assert!(result.contains(&id3));
    }

    #[test]
    fn query_and() {
        let (_dir, storage) = test_storage();
        let id1 = Ulid::new();
        let id2 = Ulid::new();

        MetadataIndex::insert(
            &storage,
            "col",
            &id1,
            &HashMap::from([("color".into(), "red".into()), ("size".into(), "s".into())]),
        )
        .unwrap();
        MetadataIndex::insert(
            &storage,
            "col",
            &id2,
            &HashMap::from([("color".into(), "red".into()), ("size".into(), "m".into())]),
        )
        .unwrap();

        let result = MetadataIndex::query(
            &storage,
            "col",
            &MetadataFilter::And(vec![
                MetadataFilter::Eq("color".into(), "red".into()),
                MetadataFilter::Eq("size".into(), "s".into()),
            ]),
        )
        .unwrap();
        assert_eq!(result.len(), 1);
        assert!(result.contains(&id1));
    }

    #[test]
    fn query_or() {
        let (_dir, storage) = test_storage();
        let id1 = Ulid::new();
        let id2 = Ulid::new();
        let id3 = Ulid::new();

        MetadataIndex::insert(&storage, "col", &id1, &HashMap::from([("color".into(), "red".into())])).unwrap();
        MetadataIndex::insert(&storage, "col", &id2, &HashMap::from([("color".into(), "blue".into())])).unwrap();
        MetadataIndex::insert(&storage, "col", &id3, &HashMap::from([("color".into(), "green".into())])).unwrap();

        let result = MetadataIndex::query(
            &storage,
            "col",
            &MetadataFilter::Or(vec![
                MetadataFilter::Eq("color".into(), "red".into()),
                MetadataFilter::Eq("color".into(), "green".into()),
            ]),
        )
        .unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.contains(&id1));
        assert!(result.contains(&id3));
    }

    #[test]
    fn remove_and_query() {
        let (_dir, storage) = test_storage();
        let id1 = Ulid::new();
        let meta = HashMap::from([("k".into(), "v".into())]);

        MetadataIndex::insert(&storage, "col", &id1, &meta).unwrap();
        MetadataIndex::remove(&storage, "col", &id1, &meta).unwrap();

        let result = MetadataIndex::query(
            &storage,
            "col",
            &MetadataFilter::Eq("k".into(), "v".into()),
        )
        .unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn empty_and_filter() {
        let (_dir, storage) = test_storage();
        let result = MetadataIndex::query(
            &storage,
            "col",
            &MetadataFilter::And(vec![]),
        )
        .unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn empty_or_filter() {
        let (_dir, storage) = test_storage();
        let result = MetadataIndex::query(
            &storage,
            "col",
            &MetadataFilter::Or(vec![]),
        )
        .unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn query_nonexistent_key() {
        let (_dir, storage) = test_storage();
        let result = MetadataIndex::query(
            &storage,
            "col",
            &MetadataFilter::Eq("no_key".into(), "no_val".into()),
        )
        .unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn in_with_empty_values() {
        let (_dir, storage) = test_storage();
        let id = Ulid::new();
        MetadataIndex::insert(
            &storage,
            "col",
            &id,
            &HashMap::from([("k".into(), "v".into())]),
        )
        .unwrap();

        let result = MetadataIndex::query(
            &storage,
            "col",
            &MetadataFilter::In("k".into(), vec![]),
        )
        .unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn and_with_single_filter() {
        let (_dir, storage) = test_storage();
        let id = Ulid::new();
        MetadataIndex::insert(
            &storage,
            "col",
            &id,
            &HashMap::from([("k".into(), "v".into())]),
        )
        .unwrap();

        let result = MetadataIndex::query(
            &storage,
            "col",
            &MetadataFilter::And(vec![MetadataFilter::Eq("k".into(), "v".into())]),
        )
        .unwrap();
        assert_eq!(result.len(), 1);
        assert!(result.contains(&id));
    }

    #[test]
    fn conflicting_and_returns_empty() {
        let (_dir, storage) = test_storage();
        let id = Ulid::new();
        MetadataIndex::insert(
            &storage,
            "col",
            &id,
            &HashMap::from([("k".into(), "v1".into())]),
        )
        .unwrap();

        let result = MetadataIndex::query(
            &storage,
            "col",
            &MetadataFilter::And(vec![
                MetadataFilter::Eq("k".into(), "v1".into()),
                MetadataFilter::Eq("k".into(), "v2".into()),
            ]),
        )
        .unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn nested_and_or_filters() {
        let (_dir, storage) = test_storage();
        let id1 = Ulid::new();
        let id2 = Ulid::new();
        let id3 = Ulid::new();

        MetadataIndex::insert(
            &storage,
            "col",
            &id1,
            &HashMap::from([("color".into(), "red".into()), ("size".into(), "s".into())]),
        )
        .unwrap();
        MetadataIndex::insert(
            &storage,
            "col",
            &id2,
            &HashMap::from([("color".into(), "blue".into()), ("size".into(), "m".into())]),
        )
        .unwrap();
        MetadataIndex::insert(
            &storage,
            "col",
            &id3,
            &HashMap::from([("color".into(), "red".into()), ("size".into(), "m".into())]),
        )
        .unwrap();

        // (color=red AND size=m) OR (color=blue)
        let result = MetadataIndex::query(
            &storage,
            "col",
            &MetadataFilter::Or(vec![
                MetadataFilter::And(vec![
                    MetadataFilter::Eq("color".into(), "red".into()),
                    MetadataFilter::Eq("size".into(), "m".into()),
                ]),
                MetadataFilter::Eq("color".into(), "blue".into()),
            ]),
        )
        .unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.contains(&id2));
        assert!(result.contains(&id3));
    }

    #[test]
    fn remove_nonexistent_entry_is_ok() {
        let (_dir, storage) = test_storage();
        let id = Ulid::new();
        // Remove from a key that was never inserted — should not error
        MetadataIndex::remove(
            &storage,
            "col",
            &id,
            &HashMap::from([("k".into(), "v".into())]),
        )
        .unwrap();
    }

    #[test]
    fn duplicate_insert_same_id() {
        let (_dir, storage) = test_storage();
        let id = Ulid::new();
        let meta = HashMap::from([("k".into(), "v".into())]);

        MetadataIndex::insert(&storage, "col", &id, &meta).unwrap();
        MetadataIndex::insert(&storage, "col", &id, &meta).unwrap();

        let result = MetadataIndex::query(
            &storage,
            "col",
            &MetadataFilter::Eq("k".into(), "v".into()),
        )
        .unwrap();
        // ID appears in posting list twice but HashSet deduplicates
        assert_eq!(result.len(), 1);
        assert!(result.contains(&id));
    }
}
