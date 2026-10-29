# C API Reference

C function reference for the `mast-ffi` crate. All functions use C-compatible calling conventions (`extern "C"`).

## Types

### MastHandle

Opaque handle to a MAST instance. Created with `mast_open()`, freed with `mast_close()`.

### MastFfiError

```c
typedef struct {
    int code;           // Error category code
    char* message;      // Heap-allocated error message
} MastFfiError;
```

Error codes:

| Code | Category |
|------|----------|
| 1 | Storage |
| 2 | Index |
| 3 | Embedder |
| 4 | NotFound |
| 5 | InvalidInput |
| 6 | Serialization |
| 7 | Compaction |
| 8 | Graph |
| 9 | Snapshot |

### MastEmbedderVtable

```c
typedef struct {
    float* (*embed)(void* user_data, const char* text,
                    size_t* out_len, MastFfiError** err);
    float** (*embed_batch)(void* user_data, const char** texts,
                           size_t count, size_t* out_lens, MastFfiError** err);
    size_t (*dimensions)(void* user_data);
    void* user_data;
} MastEmbedderVtable;
```

- `embed` is required. Must return a heap-allocated `float*` of length `*out_len`. MAST takes ownership.
- `embed_batch` is optional. If `NULL`, MAST calls `embed` sequentially.
- `dimensions` is optional. Returns 0 if `NULL`.

### MastMetadata

```c
typedef struct {
    const char** keys;
    const char** values;
    size_t count;
} MastMetadata;
```

### MastStoreResult

```c
typedef struct {
    char* id;           // ULID string (heap-allocated)
} MastStoreResult;
```

### MastRecallEntry

```c
typedef struct {
    char* id;           // ULID string
    char* content;      // Memory content
    float score;        // Relevance score
} MastRecallEntry;
```

### MastRecallResult

```c
typedef struct {
    MastRecallEntry* entries;
    size_t count;
} MastRecallResult;
```

### MastCollectionInfoEntry

```c
typedef struct {
    char* name;
    size_t dimensions;
    uint64_t memory_count;
} MastCollectionInfoEntry;
```

### MastInfoResult

```c
typedef struct {
    MastCollectionInfoEntry* collections;
    size_t count;
} MastInfoResult;
```

### MastSnapshotResult

```c
typedef struct {
    uint64_t memory_count;
    uint64_t edge_count;
} MastSnapshotResult;
```

### MastRestoreResult

```c
typedef struct {
    uint64_t memory_count;
    uint64_t edge_count;
} MastRestoreResult;
```

---

## Functions

### Lifecycle

#### mast_open

```c
MastHandle* mast_open(const char* path, MastFfiError** err);
```

Opens or creates a database. Returns `NULL` on error.

#### mast_close

```c
void mast_close(MastHandle* handle);
```

Flushes indexes and frees the handle. Safe to call with `NULL`.

---

### Store and Recall

#### mast_store

```c
MastStoreResult* mast_store(
    MastHandle* handle,
    const char* collection,
    const char* content,
    const MastEmbedderVtable* embedder,
    const MastMetadata* metadata,   // NULL for no metadata
    const char* tier,               // NULL defaults to "active"
    int64_t ttl,
    bool has_ttl,                   // false to ignore ttl
    int64_t created_at,             // when the memory happened, unix seconds
    bool has_created_at,            // false = now
    MastFfiError** err
);
```

Returns `NULL` on error. Free with `mast_free_store_result()`.

#### mast_recall

```c
MastRecallResult* mast_recall(
    MastHandle* handle,
    const char* collection,
    const char* query,              // NULL for no query
    const MastEmbedderVtable* embedder,
    size_t limit,
    MastFfiError** err
);
```

Vector search only. Returns `NULL` on error. Free with `mast_free_recall_result()`.

#### mast_recall_full

```c
MastRecallFullResult* mast_recall_full(
    MastHandle* handle,
    const char* collection,
    const char* query,              // NULL for no query
    const char* text_query,         // NULL for no full-text query
    const MastEmbedderVtable* embedder,
    const char* search_mode,        // "vector", "fulltext", "hybrid"; NULL = "vector"
    float vector_weight,            // hybrid only
    float text_weight,              // hybrid only
    size_t limit,
    const char* filter_json,        // NULL or "" for no filter
    int64_t reference_time,         // when the question is asked, unix seconds
    bool has_reference_time,        // false = now
    MastFfiError** err
);
```

Phrases like "two weeks ago" in `query` count back from `reference_time`. Free with `mast_free_recall_full_result()`.

---

### CRUD

#### mast_get

```c
bool mast_get(
    MastHandle* handle,
    const char* collection,
    const char* id,                 // ULID string
    char** out_content,             // receives content string (free with mast_free_string)
    MastFfiError** err
);
```

Returns `true` if found. Content written to `*out_content`.

#### mast_delete

```c
bool mast_delete(
    MastHandle* handle,
    const char* collection,
    const char* id,
    MastFfiError** err
);
```

Returns `true` on success.

#### mast_info

```c
MastInfoResult* mast_info(MastHandle* handle, MastFfiError** err);
```

Returns all collections. Free with `mast_free_info_result()`.

#### mast_vacuum

```c
bool mast_vacuum(
    MastHandle* handle,
    const char* collection,
    size_t* out_expired,
    MastFfiError** err
);
```

Removes expired memories. Count written to `*out_expired`.

---

### Graph

#### mast_relate

```c
bool mast_relate(
    MastHandle* handle,
    const char* collection,
    const char* source,
    const char* target,
    const char* relation,
    float weight,
    int64_t valid_from,             // when the fact started being true, unix seconds
    bool has_valid_from,
    int64_t valid_to,               // when it stopped being true, exclusive
    bool has_valid_to,
    char** out_id,                  // receives edge ID (free with mast_free_string)
    MastFfiError** err
);
```

An edge with the same source, target, relation and `valid_from` is updated in place.

#### mast_unrelate

```c
bool mast_unrelate(
    MastHandle* handle,
    const char* collection,
    const char* source,
    const char* target,
    const char* relation,
    MastFfiError** err
);
```

Removes every edge for this source, target and relation, across all validity intervals.

#### mast_invalidate

```c
bool mast_invalidate(
    MastHandle* handle,
    const char* collection,
    const char* source,
    const char* target,
    const char* relation,
    int64_t ended_at,               // when the fact stopped being true, unix seconds
    bool has_ended_at,              // false = now
    size_t* out_closed,             // receives the number of edges closed; may be NULL
    MastFfiError** err
);
```

Sets `valid_to` on the open edges for this fact. Fails if no edge is open.

#### mast_traverse

```c
MastGraphResult* mast_traverse(
    MastHandle* handle,
    const char* collection,
    const char* start,
    size_t max_depth,
    const char* relation_filter,    // NULL for all relations
    float min_weight,
    bool has_min_weight,
    int64_t as_of,                  // follow only edges valid at this time
    bool has_as_of,                 // false = all edges
    MastFfiError** err
);
```

Each `MastEdgeEntry` carries `valid_from`/`has_valid_from` and `valid_to`/`has_valid_to`. Free with `mast_free_graph_result()`.

---

### Snapshots

#### mast_snapshot

```c
MastSnapshotResult* mast_snapshot(
    MastHandle* handle,
    const char* collection,
    const char* output_path,
    MastFfiError** err
);
```

Free with `mast_free_snapshot_result()`.

#### mast_restore

```c
MastRestoreResult* mast_restore(
    MastHandle* handle,
    const char* input_path,
    MastFfiError** err
);
```

Free with `mast_free_restore_result()`.

---

### Free Functions

```c
void mast_free_error(MastFfiError* err);
void mast_free_string(char* s);
void mast_free_store_result(MastStoreResult* result);
void mast_free_recall_result(MastRecallResult* result);
void mast_free_info_result(MastInfoResult* result);
void mast_free_snapshot_result(MastSnapshotResult* result);
void mast_free_restore_result(MastRestoreResult* result);
```

All free functions are safe to call with `NULL`.
