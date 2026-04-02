# C FFI Guide

MAST provides a C-compatible FFI via the `mast-ffi` crate. This enables integration with any language that can call C functions (Go, Swift, Ruby, etc.).

## Building

```bash
cd crates/mast-ffi
cargo build --release
# Produces: target/release/libmast_ffi.dylib (macOS) or libmast_ffi.so (Linux)
```

## Core Concepts

### MastHandle

An opaque pointer to the MAST instance. Created with `mast_open()`, freed with `mast_close()`.

### Error Handling

All functions that can fail accept a `MastFfiError**` out-parameter. On error, the pointer is set to an allocated error struct:

```c
typedef struct {
    int code;       // Error category (see table below)
    char* message;  // Human-readable error message
} MastFfiError;
```

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

Always check the error pointer after each call and free it with `mast_free_error()`.

### Embedder Callbacks

The FFI uses a vtable of function pointers for embedding:

```c
typedef struct {
    // Required: embed a single text
    float* (*embed)(
        void* user_data,
        const char* text,
        size_t* out_len,
        MastFfiError** err
    );

    // Optional: batch embed (falls back to sequential if NULL)
    float** (*embed_batch)(
        void* user_data,
        const char** texts,
        size_t count,
        size_t* out_lens,
        MastFfiError** err
    );

    // Optional: return dimensions
    size_t (*dimensions)(void* user_data);

    // Opaque pointer passed to all callbacks
    void* user_data;
} MastEmbedderVtable;
```

The `embed` callback must return a heap-allocated `float*` array. MAST takes ownership via `Vec::from_raw_parts`.

## Usage Example

```c
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// Forward declarations (from mast-ffi)
typedef struct MastHandle MastHandle;
typedef struct MastFfiError MastFfiError;
typedef struct MastEmbedderVtable MastEmbedderVtable;
typedef struct MastStoreResult MastStoreResult;
typedef struct MastRecallResult MastRecallResult;

extern MastHandle* mast_open(const char* path, MastFfiError** err);
extern void mast_close(MastHandle* handle);
extern MastStoreResult* mast_store(
    MastHandle* handle, const char* collection, const char* content,
    const MastEmbedderVtable* embedder, const void* metadata,
    const char* tier, long long ttl, int has_ttl, MastFfiError** err
);
extern void mast_free_error(MastFfiError* err);
extern void mast_free_store_result(MastStoreResult* result);

int main() {
    MastFfiError* err = NULL;

    // Open database
    MastHandle* db = mast_open("test.mast", &err);
    if (err) {
        fprintf(stderr, "Error: %s\n", err->message);
        mast_free_error(err);
        return 1;
    }

    // Set up embedder vtable
    MastEmbedderVtable vtable = {
        .embed = my_embed_fn,
        .embed_batch = NULL,  // optional
        .dimensions = my_dims_fn,
        .user_data = my_context,
    };

    // Store a memory
    MastStoreResult* result = mast_store(
        db, "notes", "hello world",
        &vtable, NULL, "active", 0, 0, &err
    );
    if (err) {
        fprintf(stderr, "Store error: %s\n", err->message);
        mast_free_error(err);
    } else {
        printf("Stored: %s\n", result->id);
        mast_free_store_result(result);
    }

    mast_close(db);
    return 0;
}
```

## API Functions

### Lifecycle

```c
MastHandle* mast_open(const char* path, MastFfiError** err);
void mast_close(MastHandle* handle);
```

### Store and Recall

```c
MastStoreResult* mast_store(
    MastHandle* handle, const char* collection, const char* content,
    const MastEmbedderVtable* embedder, const MastMetadata* metadata,
    const char* tier, long long ttl, int has_ttl, MastFfiError** err
);

MastRecallResult* mast_recall(
    MastHandle* handle, const char* collection, const char* query,
    const MastEmbedderVtable* embedder, size_t limit, MastFfiError** err
);
```

### CRUD

```c
int mast_get(MastHandle* handle, const char* collection,
             const char* id, char** out_content, MastFfiError** err);
int mast_delete(MastHandle* handle, const char* collection,
                const char* id, MastFfiError** err);
MastInfoResult* mast_info(MastHandle* handle, MastFfiError** err);
int mast_vacuum(MastHandle* handle, const char* collection,
                size_t* out_expired, MastFfiError** err);
```

### Graph

```c
int mast_relate(MastHandle* handle, const char* collection,
                const char* source, const char* target, const char* relation,
                float weight, char** out_id, MastFfiError** err);
int mast_unrelate(MastHandle* handle, const char* collection,
                  const char* source, const char* target, const char* relation,
                  MastFfiError** err);
```

### Snapshots

```c
MastSnapshotResult* mast_snapshot(MastHandle* handle, const char* collection,
                                   const char* output_path, MastFfiError** err);
MastRestoreResult* mast_restore(MastHandle* handle, const char* input_path,
                                 MastFfiError** err);
```

### Free Functions

Every allocated result must be freed:

```c
void mast_free_error(MastFfiError* err);
void mast_free_string(char* s);
void mast_free_store_result(MastStoreResult* result);
void mast_free_recall_result(MastRecallResult* result);
void mast_free_info_result(MastInfoResult* result);
void mast_free_snapshot_result(MastSnapshotResult* result);
void mast_free_restore_result(MastRestoreResult* result);
```

## Metadata

Pass metadata via the `MastMetadata` struct:

```c
typedef struct {
    const char** keys;
    const char** values;
    size_t count;
} MastMetadata;
```

```c
const char* keys[] = {"source", "topic"};
const char* values[] = {"conversation", "preferences"};
MastMetadata meta = { .keys = keys, .values = values, .count = 2 };

mast_store(db, "notes", "content", &vtable, &meta, "active", 0, 0, &err);
```

Pass `NULL` for no metadata.
