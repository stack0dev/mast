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
    char** out_id,                  // receives edge ID (free with mast_free_string)
    MastFfiError** err
);
```

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
