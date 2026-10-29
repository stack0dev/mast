use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;

use mast_core::compact::{CompactedMemory, Compactor};
use mast_core::config::MastConfig;
use mast_core::error::MastError;
use mast_core::types::*;
use mast_core::Mast;

// --- Error handling ---

#[repr(C)]
pub struct MastFfiError {
    pub code: i32,
    pub message: *mut c_char,
}

fn error_code(e: &MastError) -> i32 {
    match e {
        MastError::Storage(_) => 1,
        MastError::Index(_) => 2,
        MastError::Embedder(_) => 3,
        MastError::NotFound(_) => 4,
        MastError::InvalidInput(_) => 5,
        MastError::Serialization(_) => 6,
        MastError::Compaction(_) => 7,
        MastError::Graph(_) => 8,
        MastError::Snapshot(_) => 9,
    }
}

fn set_error(err_out: *mut *mut MastFfiError, e: MastError) {
    if err_out.is_null() {
        return;
    }
    let msg = CString::new(e.to_string()).unwrap_or_else(|_| CString::new("unknown error").unwrap());
    let ffi_err = Box::new(MastFfiError {
        code: error_code(&e),
        message: msg.into_raw(),
    });
    unsafe {
        *err_out = Box::into_raw(ffi_err);
    }
}

/// Free an error returned by a mast function.
#[no_mangle]
pub extern "C" fn mast_free_error(err: *mut MastFfiError) {
    if err.is_null() {
        return;
    }
    unsafe {
        let e = Box::from_raw(err);
        if !e.message.is_null() {
            drop(CString::from_raw(e.message));
        }
    }
}

// --- Handle ---

pub struct MastHandle {
    mast: Mast,
    runtime: tokio::runtime::Runtime,
}

// --- Embedder callback vtable ---

#[repr(C)]
pub struct MastEmbedderVtable {
    pub embed: Option<
        unsafe extern "C" fn(
            user_data: *mut std::ffi::c_void,
            text: *const c_char,
            out_len: *mut usize,
            err: *mut *mut MastFfiError,
        ) -> *mut f32,
    >,
    pub embed_batch: Option<
        unsafe extern "C" fn(
            user_data: *mut std::ffi::c_void,
            texts: *const *const c_char,
            count: usize,
            out_lens: *mut usize,
            err: *mut *mut MastFfiError,
        ) -> *mut *mut f32,
    >,
    pub dimensions: Option<unsafe extern "C" fn(user_data: *mut std::ffi::c_void) -> usize>,
    pub user_data: *mut std::ffi::c_void,
}

unsafe impl Send for MastEmbedderVtable {}
unsafe impl Sync for MastEmbedderVtable {}

struct FfiEmbedder {
    vtable: MastEmbedderVtable,
}

unsafe impl Send for FfiEmbedder {}
unsafe impl Sync for FfiEmbedder {}

#[async_trait::async_trait]
impl mast_core::embed::Embedder for FfiEmbedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, MastError> {
        let embed_fn = self
            .vtable
            .embed
            .ok_or_else(|| MastError::Embedder("embed callback not set".into()))?;
        let c_text = CString::new(text)
            .map_err(|e| MastError::Embedder(format!("invalid text: {e}")))?;
        let mut out_len: usize = 0;
        let mut err_ptr: *mut MastFfiError = ptr::null_mut();
        let data = unsafe { embed_fn(self.vtable.user_data, c_text.as_ptr(), &mut out_len, &mut err_ptr) };
        if !err_ptr.is_null() {
            let msg = unsafe {
                let e = Box::from_raw(err_ptr);
                let s = if !e.message.is_null() {
                    CStr::from_ptr(e.message).to_string_lossy().into_owned()
                } else {
                    "embedder error".to_string()
                };
                if !e.message.is_null() {
                    drop(CString::from_raw(e.message));
                }
                s
            };
            return Err(MastError::Embedder(msg));
        }
        if data.is_null() || out_len == 0 {
            return Err(MastError::Embedder("embed returned null".into()));
        }
        let vec = unsafe { Vec::from_raw_parts(data, out_len, out_len) };
        Ok(vec)
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, MastError> {
        // Fall back to sequential embed if batch not provided
        let batch_fn = match self.vtable.embed_batch {
            Some(f) => f,
            None => {
                let mut results = Vec::with_capacity(texts.len());
                for text in texts {
                    results.push(self.embed(text).await?);
                }
                return Ok(results);
            }
        };
        let c_texts: Vec<CString> = texts
            .iter()
            .map(|t| CString::new(t.as_str()).unwrap())
            .collect();
        let c_ptrs: Vec<*const c_char> = c_texts.iter().map(|t| t.as_ptr()).collect();
        let mut out_lens = vec![0usize; texts.len()];
        let mut err_ptr: *mut MastFfiError = ptr::null_mut();
        let data = unsafe {
            batch_fn(
                self.vtable.user_data,
                c_ptrs.as_ptr(),
                texts.len(),
                out_lens.as_mut_ptr(),
                &mut err_ptr,
            )
        };
        if !err_ptr.is_null() {
            let msg = unsafe {
                let e = Box::from_raw(err_ptr);
                let s = if !e.message.is_null() {
                    CStr::from_ptr(e.message).to_string_lossy().into_owned()
                } else {
                    "embedder batch error".to_string()
                };
                if !e.message.is_null() {
                    drop(CString::from_raw(e.message));
                }
                s
            };
            return Err(MastError::Embedder(msg));
        }
        if data.is_null() {
            return Err(MastError::Embedder("embed_batch returned null".into()));
        }
        let mut results = Vec::with_capacity(texts.len());
        for (i, &len) in out_lens.iter().enumerate() {
            let vec_ptr = unsafe { *data.add(i) };
            if vec_ptr.is_null() {
                return Err(MastError::Embedder(format!("null vector at index {i}")));
            }
            let vec = unsafe { Vec::from_raw_parts(vec_ptr, len, len) };
            results.push(vec);
        }
        unsafe {
            // Free the outer array (not the inner vecs, those are now owned)
            drop(Vec::from_raw_parts(data, texts.len(), texts.len()));
        }
        Ok(results)
    }

    fn dimensions(&self) -> usize {
        match self.vtable.dimensions {
            Some(f) => unsafe { f(self.vtable.user_data) },
            None => 0,
        }
    }
}

// --- Metadata helpers ---

#[repr(C)]
pub struct MastMetadata {
    pub keys: *const *const c_char,
    pub values: *const *const c_char,
    pub count: usize,
}

fn metadata_from_ffi(meta: *const MastMetadata) -> HashMap<String, String> {
    if meta.is_null() {
        return HashMap::new();
    }
    let m = unsafe { &*meta };
    if m.count == 0 || m.keys.is_null() || m.values.is_null() {
        return HashMap::new();
    }
    let mut map = HashMap::new();
    for i in 0..m.count {
        unsafe {
            let k = CStr::from_ptr(*m.keys.add(i)).to_string_lossy().into_owned();
            let v = CStr::from_ptr(*m.values.add(i)).to_string_lossy().into_owned();
            map.insert(k, v);
        }
    }
    map
}

// --- Result types ---

#[repr(C)]
pub struct MastStoreResult {
    pub id: *mut c_char,
}

#[repr(C)]
pub struct MastRecallEntry {
    pub id: *mut c_char,
    pub content: *mut c_char,
    pub score: f32,
}

#[repr(C)]
pub struct MastRecallResult {
    pub entries: *mut MastRecallEntry,
    pub count: usize,
}

#[repr(C)]
pub struct MastCollectionInfoEntry {
    pub name: *mut c_char,
    pub dimensions: usize,
    pub memory_count: u64,
}

#[repr(C)]
pub struct MastInfoResult {
    pub collections: *mut MastCollectionInfoEntry,
    pub count: usize,
}

#[repr(C)]
pub struct MastSnapshotResult {
    pub memory_count: u64,
    pub edge_count: u64,
}

#[repr(C)]
pub struct MastRestoreResult {
    pub memory_count: u64,
    pub edge_count: u64,
}

// --- List result types ---

#[repr(C)]
pub struct MastMemoryEntry {
    pub id: *mut c_char,
    pub content: *mut c_char,
    pub tier: *mut c_char,
    pub created_at: i64,
    pub access_count: u64,
    pub metadata_json: *mut c_char,
}

#[repr(C)]
pub struct MastListResult {
    pub entries: *mut MastMemoryEntry,
    pub count: usize,
}

// --- Full recall result types ---

#[repr(C)]
pub struct MastRecallFullEntry {
    pub id: *mut c_char,
    pub content: *mut c_char,
    pub score: f32,
    pub tier: *mut c_char,
    pub created_at: i64,
    pub access_count: u64,
    pub metadata_json: *mut c_char,
}

#[repr(C)]
pub struct MastRecallFullResult {
    pub entries: *mut MastRecallFullEntry,
    pub count: usize,
}

// --- Compact types ---

#[repr(C)]
pub struct MastCompactorVtable {
    pub synthesize: Option<
        unsafe extern "C" fn(
            user_data: *mut std::ffi::c_void,
            contents: *const *const c_char,
            count: usize,
            err: *mut *mut MastFfiError,
        ) -> *mut c_char,
    >,
    pub user_data: *mut std::ffi::c_void,
}

unsafe impl Send for MastCompactorVtable {}
unsafe impl Sync for MastCompactorVtable {}

struct FfiCompactor {
    vtable: MastCompactorVtable,
}

unsafe impl Send for FfiCompactor {}
unsafe impl Sync for FfiCompactor {}

#[async_trait::async_trait]
impl Compactor for FfiCompactor {
    async fn synthesize(&self, memories: &[Memory]) -> Result<CompactedMemory, MastError> {
        let synth_fn = self
            .vtable
            .synthesize
            .ok_or_else(|| MastError::Compaction("synthesize callback not set".into()))?;

        let c_contents: Vec<CString> = memories
            .iter()
            .map(|m| CString::new(m.content.as_str()).unwrap())
            .collect();
        let c_ptrs: Vec<*const c_char> = c_contents.iter().map(|c| c.as_ptr()).collect();

        let mut err_ptr: *mut MastFfiError = ptr::null_mut();
        let result = unsafe {
            synth_fn(
                self.vtable.user_data,
                c_ptrs.as_ptr(),
                memories.len(),
                &mut err_ptr,
            )
        };

        if !err_ptr.is_null() {
            let msg = unsafe {
                let e = Box::from_raw(err_ptr);
                let s = if !e.message.is_null() {
                    CStr::from_ptr(e.message).to_string_lossy().into_owned()
                } else {
                    "compactor error".to_string()
                };
                if !e.message.is_null() {
                    drop(CString::from_raw(e.message));
                }
                s
            };
            return Err(MastError::Compaction(msg));
        }

        if result.is_null() {
            return Err(MastError::Compaction("synthesize returned null".into()));
        }

        let content = unsafe { CStr::from_ptr(result).to_string_lossy().into_owned() };
        unsafe { drop(CString::from_raw(result)) };

        Ok(CompactedMemory {
            content,
            metadata: HashMap::new(),
        })
    }
}

#[repr(C)]
pub struct MastCompactResult {
    pub source_count: usize,
    pub compacted_count: usize,
    pub new_ids: *mut *mut c_char,
    pub new_ids_count: usize,
    pub source_ids: *mut *mut c_char,
    pub source_ids_count: usize,
}

// --- Graph result types ---

#[repr(C)]
pub struct MastEdgeEntry {
    pub id: *mut c_char,
    pub source: *mut c_char,
    pub target: *mut c_char,
    pub relation: *mut c_char,
    pub weight: f32,
    pub created_at: i64,
    pub updated_at: i64,
    pub valid_from: i64,
    pub has_valid_from: bool,
    pub valid_to: i64,
    pub has_valid_to: bool,
}

#[repr(C)]
pub struct MastGraphResult {
    pub edges: *mut MastEdgeEntry,
    pub count: usize,
}

// --- Core API ---

#[no_mangle]
pub extern "C" fn mast_open(
    path: *const c_char,
    err: *mut *mut MastFfiError,
) -> *mut MastHandle {
    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(e) => {
            set_error(err, MastError::InvalidInput(format!("invalid path: {e}")));
            return ptr::null_mut();
        }
    };

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            set_error(err, MastError::Storage(format!("failed to create runtime: {e}")));
            return ptr::null_mut();
        }
    };

    let config = MastConfig::default().with_db_path(path_str);
    match Mast::open(config) {
        Ok(mast) => Box::into_raw(Box::new(MastHandle { mast, runtime })),
        Err(e) => {
            set_error(err, e);
            ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "C" fn mast_close(handle: *mut MastHandle) {
    if handle.is_null() {
        return;
    }
    unsafe {
        let mut h = Box::from_raw(handle);
        let _ = h.mast.close();
    }
}

#[no_mangle]
pub extern "C" fn mast_store(
    handle: *mut MastHandle,
    collection: *const c_char,
    content: *const c_char,
    embedder: *const MastEmbedderVtable,
    metadata: *const MastMetadata,
    tier: *const c_char,
    ttl: i64,
    has_ttl: bool,
    created_at: i64,
    has_created_at: bool,
    err: *mut *mut MastFfiError,
) -> *mut MastStoreResult {
    let h = unsafe { &mut *handle };
    let col = unsafe { CStr::from_ptr(collection) }.to_string_lossy().into_owned();
    let cnt = unsafe { CStr::from_ptr(content) }.to_string_lossy().into_owned();
    let meta = metadata_from_ffi(metadata);

    let tier_val = if tier.is_null() {
        Tier::Active
    } else {
        let t = unsafe { CStr::from_ptr(tier) }.to_string_lossy();
        t.parse().unwrap_or(Tier::Active)
    };

    let ttl_val = if has_ttl { Some(ttl) } else { None };

    let embedder_vtable = unsafe { std::ptr::read(embedder) };
    let ffi_embedder = FfiEmbedder { vtable: embedder_vtable };

    let request = StoreRequest {
        collection: col,
        content: cnt,
        embedding: None,
        metadata: meta,
        tier: tier_val,
        ttl: ttl_val,
        created_at: has_created_at.then_some(created_at),
    };

    match h.runtime.block_on(h.mast.store(request, &ffi_embedder)) {
        Ok(memory) => {
            let id = CString::new(memory.id.to_string()).unwrap();
            Box::into_raw(Box::new(MastStoreResult {
                id: id.into_raw(),
            }))
        }
        Err(e) => {
            set_error(err, e);
            ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "C" fn mast_recall(
    handle: *mut MastHandle,
    collection: *const c_char,
    query: *const c_char,
    embedder: *const MastEmbedderVtable,
    limit: usize,
    err: *mut *mut MastFfiError,
) -> *mut MastRecallResult {
    let h = unsafe { &mut *handle };
    let col = unsafe { CStr::from_ptr(collection) }.to_string_lossy().into_owned();

    let query_str = if query.is_null() {
        None
    } else {
        Some(unsafe { CStr::from_ptr(query) }.to_string_lossy().into_owned())
    };

    let embedder_vtable = unsafe { std::ptr::read(embedder) };
    let ffi_embedder = FfiEmbedder { vtable: embedder_vtable };

    let request = RecallRequest {
        collection: col,
        query: query_str,
        query_embedding: None,
        text_query: None,
        search_mode: SearchMode::Vector,
        limit,
        filter: None,
        tier_weights: None,
        time_decay: None,
        reference_time: None,
        time_anchor: None,
    };

    match h.runtime.block_on(h.mast.recall(request, &ffi_embedder)) {
        Ok(results) => {
            let count = results.len();
            let mut entries: Vec<MastRecallEntry> = results
                .into_iter()
                .map(|r| {
                    let id = CString::new(r.memory.id.to_string()).unwrap();
                    let content = CString::new(r.memory.content).unwrap();
                    MastRecallEntry {
                        id: id.into_raw(),
                        content: content.into_raw(),
                        score: r.score,
                    }
                })
                .collect();
            let ptr = entries.as_mut_ptr();
            std::mem::forget(entries);
            Box::into_raw(Box::new(MastRecallResult {
                entries: ptr,
                count,
            }))
        }
        Err(e) => {
            set_error(err, e);
            ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "C" fn mast_get(
    handle: *mut MastHandle,
    collection: *const c_char,
    id: *const c_char,
    out_content: *mut *mut c_char,
    err: *mut *mut MastFfiError,
) -> bool {
    let h = unsafe { &mut *handle };
    let col = unsafe { CStr::from_ptr(collection) }.to_string_lossy().into_owned();
    let id_str = unsafe { CStr::from_ptr(id) }.to_string_lossy();
    let ulid: ulid::Ulid = match id_str.parse() {
        Ok(u) => u,
        Err(e) => {
            set_error(err, MastError::InvalidInput(format!("invalid ULID: {e}")));
            return false;
        }
    };

    match h.mast.get(&col, &ulid) {
        Ok(Some(memory)) => {
            if !out_content.is_null() {
                let c = CString::new(memory.content).unwrap();
                unsafe { *out_content = c.into_raw() };
            }
            true
        }
        Ok(None) => false,
        Err(e) => {
            set_error(err, e);
            false
        }
    }
}

#[no_mangle]
pub extern "C" fn mast_delete(
    handle: *mut MastHandle,
    collection: *const c_char,
    id: *const c_char,
    err: *mut *mut MastFfiError,
) -> bool {
    let h = unsafe { &mut *handle };
    let col = unsafe { CStr::from_ptr(collection) }.to_string_lossy().into_owned();
    let id_str = unsafe { CStr::from_ptr(id) }.to_string_lossy();
    let ulid: ulid::Ulid = match id_str.parse() {
        Ok(u) => u,
        Err(e) => {
            set_error(err, MastError::InvalidInput(format!("invalid ULID: {e}")));
            return false;
        }
    };

    match h.mast.delete(&col, &ulid) {
        Ok(()) => true,
        Err(e) => {
            set_error(err, e);
            false
        }
    }
}

#[no_mangle]
pub extern "C" fn mast_info(
    handle: *mut MastHandle,
    err: *mut *mut MastFfiError,
) -> *mut MastInfoResult {
    let h = unsafe { &mut *handle };
    match h.mast.info() {
        Ok(collections) => {
            let count = collections.len();
            let mut entries: Vec<MastCollectionInfoEntry> = collections
                .into_iter()
                .map(|c| {
                    let name = CString::new(c.name).unwrap();
                    MastCollectionInfoEntry {
                        name: name.into_raw(),
                        dimensions: c.dimensions,
                        memory_count: c.memory_count,
                    }
                })
                .collect();
            let ptr = entries.as_mut_ptr();
            std::mem::forget(entries);
            Box::into_raw(Box::new(MastInfoResult {
                collections: ptr,
                count,
            }))
        }
        Err(e) => {
            set_error(err, e);
            ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "C" fn mast_vacuum(
    handle: *mut MastHandle,
    collection: *const c_char,
    out_expired: *mut usize,
    err: *mut *mut MastFfiError,
) -> bool {
    let h = unsafe { &mut *handle };
    let col = unsafe { CStr::from_ptr(collection) }.to_string_lossy().into_owned();
    match h.mast.vacuum(&col) {
        Ok(result) => {
            if !out_expired.is_null() {
                unsafe { *out_expired = result.expired_count };
            }
            true
        }
        Err(e) => {
            set_error(err, e);
            false
        }
    }
}

#[no_mangle]
pub extern "C" fn mast_relate(
    handle: *mut MastHandle,
    collection: *const c_char,
    source: *const c_char,
    target: *const c_char,
    relation: *const c_char,
    weight: f32,
    valid_from: i64,
    has_valid_from: bool,
    valid_to: i64,
    has_valid_to: bool,
    out_id: *mut *mut c_char,
    err: *mut *mut MastFfiError,
) -> bool {
    let h = unsafe { &mut *handle };
    let col = unsafe { CStr::from_ptr(collection) }.to_string_lossy().into_owned();
    let src = unsafe { CStr::from_ptr(source) }.to_string_lossy().into_owned();
    let tgt = unsafe { CStr::from_ptr(target) }.to_string_lossy().into_owned();
    let rel = unsafe { CStr::from_ptr(relation) }.to_string_lossy().into_owned();

    let request = RelateRequest {
        collection: col,
        source: src,
        target: tgt,
        relation: rel,
        weight,
        memory_ids: vec![],
        valid_from: has_valid_from.then_some(valid_from),
        valid_to: has_valid_to.then_some(valid_to),
    };

    match h.mast.relate(request) {
        Ok(id) => {
            if !out_id.is_null() {
                let c = CString::new(id.to_string()).unwrap();
                unsafe { *out_id = c.into_raw() };
            }
            true
        }
        Err(e) => {
            set_error(err, e);
            false
        }
    }
}

#[no_mangle]
pub extern "C" fn mast_unrelate(
    handle: *mut MastHandle,
    collection: *const c_char,
    source: *const c_char,
    target: *const c_char,
    relation: *const c_char,
    err: *mut *mut MastFfiError,
) -> bool {
    let h = unsafe { &mut *handle };
    let col = unsafe { CStr::from_ptr(collection) }.to_string_lossy().into_owned();
    let src = unsafe { CStr::from_ptr(source) }.to_string_lossy().into_owned();
    let tgt = unsafe { CStr::from_ptr(target) }.to_string_lossy().into_owned();
    let rel = unsafe { CStr::from_ptr(relation) }.to_string_lossy().into_owned();

    match h.mast.unrelate(&col, &src, &tgt, &rel) {
        Ok(()) => true,
        Err(e) => {
            set_error(err, e);
            false
        }
    }
}

/// Set `valid_to` on the open edges for this fact. Writes how many were
/// closed to `out_closed` when it is not null.
#[no_mangle]
pub extern "C" fn mast_invalidate(
    handle: *mut MastHandle,
    collection: *const c_char,
    source: *const c_char,
    target: *const c_char,
    relation: *const c_char,
    ended_at: i64,
    has_ended_at: bool,
    out_closed: *mut usize,
    err: *mut *mut MastFfiError,
) -> bool {
    let h = unsafe { &mut *handle };
    let request = InvalidateRequest {
        collection: unsafe { CStr::from_ptr(collection) }.to_string_lossy().into_owned(),
        source: unsafe { CStr::from_ptr(source) }.to_string_lossy().into_owned(),
        target: unsafe { CStr::from_ptr(target) }.to_string_lossy().into_owned(),
        relation: unsafe { CStr::from_ptr(relation) }.to_string_lossy().into_owned(),
        ended_at: has_ended_at.then_some(ended_at),
    };

    match h.mast.invalidate(request) {
        Ok(closed) => {
            if !out_closed.is_null() {
                unsafe { *out_closed = closed };
            }
            true
        }
        Err(e) => {
            set_error(err, e);
            false
        }
    }
}

#[no_mangle]
pub extern "C" fn mast_snapshot(
    handle: *mut MastHandle,
    collection: *const c_char,
    output_path: *const c_char,
    err: *mut *mut MastFfiError,
) -> *mut MastSnapshotResult {
    let h = unsafe { &mut *handle };
    let col = unsafe { CStr::from_ptr(collection) }.to_string_lossy().into_owned();
    let path = unsafe { CStr::from_ptr(output_path) }.to_string_lossy().into_owned();

    let mut file = match std::fs::File::create(&path) {
        Ok(f) => f,
        Err(e) => {
            set_error(err, MastError::Snapshot(format!("failed to create file: {e}")));
            return ptr::null_mut();
        }
    };

    match h.mast.snapshot(&col, &mut file) {
        Ok(result) => Box::into_raw(Box::new(MastSnapshotResult {
            memory_count: result.memory_count,
            edge_count: result.edge_count,
        })),
        Err(e) => {
            set_error(err, e);
            ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "C" fn mast_restore(
    handle: *mut MastHandle,
    input_path: *const c_char,
    merge: bool,
    err: *mut *mut MastFfiError,
) -> *mut MastRestoreResult {
    let h = unsafe { &mut *handle };
    let path = unsafe { CStr::from_ptr(input_path) }.to_string_lossy().into_owned();

    let file = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(e) => {
            set_error(err, MastError::Snapshot(format!("failed to open file: {e}")));
            return ptr::null_mut();
        }
    };
    let mut reader = std::io::BufReader::new(file);

    match h.mast.restore(&mut reader, merge) {
        Ok(result) => Box::into_raw(Box::new(MastRestoreResult {
            memory_count: result.memory_count,
            edge_count: result.edge_count,
        })),
        Err(e) => {
            set_error(err, e);
            ptr::null_mut()
        }
    }
}

// --- List API ---

#[no_mangle]
pub extern "C" fn mast_list(
    handle: *mut MastHandle,
    collection: *const c_char,
    err: *mut *mut MastFfiError,
) -> *mut MastListResult {
    let h = unsafe { &mut *handle };
    let col = unsafe { CStr::from_ptr(collection) }.to_string_lossy().into_owned();

    match h.mast.list(&col) {
        Ok(memories) => {
            let count = memories.len();
            let mut entries: Vec<MastMemoryEntry> = memories
                .into_iter()
                .map(|m| {
                    let id = CString::new(m.id.to_string()).unwrap();
                    let content = CString::new(m.content).unwrap();
                    let tier = CString::new(m.tier.to_string()).unwrap();
                    let meta_json = serde_json::to_string(&m.metadata).unwrap_or_default();
                    let meta = CString::new(meta_json).unwrap();
                    MastMemoryEntry {
                        id: id.into_raw(),
                        content: content.into_raw(),
                        tier: tier.into_raw(),
                        created_at: m.created_at,
                        access_count: m.access_count,
                        metadata_json: meta.into_raw(),
                    }
                })
                .collect();
            let ptr = entries.as_mut_ptr();
            std::mem::forget(entries);
            Box::into_raw(Box::new(MastListResult {
                entries: ptr,
                count,
            }))
        }
        Err(e) => {
            set_error(err, e);
            ptr::null_mut()
        }
    }
}

// --- Full recall API ---

fn parse_metadata_filter(json: &str) -> Result<MetadataFilter, MastError> {
    let v: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| MastError::InvalidInput(format!("invalid filter JSON: {e}")))?;
    parse_filter_value(&v)
}

fn parse_filter_value(v: &serde_json::Value) -> Result<MetadataFilter, MastError> {
    if let Some(obj) = v.as_object() {
        if let Some(val) = obj.get("eq") {
            let arr = val.as_array().ok_or_else(|| {
                MastError::InvalidInput("eq must be [key, value]".into())
            })?;
            if arr.len() != 2 {
                return Err(MastError::InvalidInput("eq must be [key, value]".into()));
            }
            let key = arr[0].as_str().unwrap_or_default().to_string();
            let value = arr[1].as_str().unwrap_or_default().to_string();
            return Ok(MetadataFilter::Eq(key, value));
        }
        if let Some(val) = obj.get("in") {
            let arr = val.as_array().ok_or_else(|| {
                MastError::InvalidInput("in must be [key, [values]]".into())
            })?;
            if arr.len() != 2 {
                return Err(MastError::InvalidInput("in must be [key, [values]]".into()));
            }
            let key = arr[0].as_str().unwrap_or_default().to_string();
            let values = arr[1]
                .as_array()
                .ok_or_else(|| MastError::InvalidInput("in values must be array".into()))?
                .iter()
                .map(|v| v.as_str().unwrap_or_default().to_string())
                .collect();
            return Ok(MetadataFilter::In(key, values));
        }
        if let Some(val) = obj.get("and") {
            let arr = val.as_array().ok_or_else(|| {
                MastError::InvalidInput("and must be array".into())
            })?;
            let filters: Result<Vec<_>, _> = arr.iter().map(parse_filter_value).collect();
            return Ok(MetadataFilter::And(filters?));
        }
        if let Some(val) = obj.get("or") {
            let arr = val.as_array().ok_or_else(|| {
                MastError::InvalidInput("or must be array".into())
            })?;
            let filters: Result<Vec<_>, _> = arr.iter().map(parse_filter_value).collect();
            return Ok(MetadataFilter::Or(filters?));
        }
    }
    Err(MastError::InvalidInput(format!(
        "unrecognized filter: {v}"
    )))
}

#[no_mangle]
pub extern "C" fn mast_recall_full(
    handle: *mut MastHandle,
    collection: *const c_char,
    query: *const c_char,
    text_query: *const c_char,
    embedder: *const MastEmbedderVtable,
    search_mode: *const c_char,
    vector_weight: f32,
    text_weight: f32,
    limit: usize,
    filter_json: *const c_char,
    reference_time: i64,
    has_reference_time: bool,
    err: *mut *mut MastFfiError,
) -> *mut MastRecallFullResult {
    let h = unsafe { &mut *handle };
    let col = unsafe { CStr::from_ptr(collection) }.to_string_lossy().into_owned();

    let query_str = if query.is_null() {
        None
    } else {
        Some(unsafe { CStr::from_ptr(query) }.to_string_lossy().into_owned())
    };

    let text_query_str = if text_query.is_null() {
        None
    } else {
        Some(unsafe { CStr::from_ptr(text_query) }.to_string_lossy().into_owned())
    };

    let mode_str = if search_mode.is_null() {
        "vector".to_string()
    } else {
        unsafe { CStr::from_ptr(search_mode) }.to_string_lossy().into_owned()
    };

    let mode = match mode_str.as_str() {
        "fulltext" => SearchMode::FullText,
        "hybrid" => SearchMode::Hybrid {
            vector_weight,
            text_weight,
        },
        _ => SearchMode::Vector,
    };

    let filter = if filter_json.is_null() {
        None
    } else {
        let json = unsafe { CStr::from_ptr(filter_json) }.to_string_lossy().into_owned();
        if json.is_empty() {
            None
        } else {
            match parse_metadata_filter(&json) {
                Ok(f) => Some(f),
                Err(e) => {
                    set_error(err, e);
                    return ptr::null_mut();
                }
            }
        }
    };

    let embedder_vtable = unsafe { std::ptr::read(embedder) };
    let ffi_embedder = FfiEmbedder { vtable: embedder_vtable };

    let request = RecallRequest {
        collection: col,
        query: query_str,
        query_embedding: None,
        text_query: text_query_str,
        search_mode: mode,
        limit,
        filter,
        tier_weights: None,
        time_decay: None,
        reference_time: has_reference_time.then_some(reference_time),
        time_anchor: None,
    };

    match h.runtime.block_on(h.mast.recall(request, &ffi_embedder)) {
        Ok(results) => {
            let count = results.len();
            let mut entries: Vec<MastRecallFullEntry> = results
                .into_iter()
                .map(|r| {
                    let id = CString::new(r.memory.id.to_string()).unwrap();
                    let content = CString::new(r.memory.content).unwrap();
                    let tier = CString::new(r.memory.tier.to_string()).unwrap();
                    let meta_json =
                        serde_json::to_string(&r.memory.metadata).unwrap_or_default();
                    let meta = CString::new(meta_json).unwrap();
                    MastRecallFullEntry {
                        id: id.into_raw(),
                        content: content.into_raw(),
                        score: r.score,
                        tier: tier.into_raw(),
                        created_at: r.memory.created_at,
                        access_count: r.memory.access_count,
                        metadata_json: meta.into_raw(),
                    }
                })
                .collect();
            let ptr = entries.as_mut_ptr();
            std::mem::forget(entries);
            Box::into_raw(Box::new(MastRecallFullResult {
                entries: ptr,
                count,
            }))
        }
        Err(e) => {
            set_error(err, e);
            ptr::null_mut()
        }
    }
}

// --- Compact API ---

#[no_mangle]
pub extern "C" fn mast_compact(
    handle: *mut MastHandle,
    collection: *const c_char,
    source_tier: *const c_char,
    target_tier: *const c_char,
    strategy: *const c_char,
    max_batch_size: usize,
    dry_run: bool,
    compactor: *const MastCompactorVtable,
    embedder: *const MastEmbedderVtable,
    err: *mut *mut MastFfiError,
) -> *mut MastCompactResult {
    let h = unsafe { &mut *handle };
    let col = unsafe { CStr::from_ptr(collection) }.to_string_lossy().into_owned();

    let src_tier: Tier = if source_tier.is_null() {
        Tier::Active
    } else {
        let t = unsafe { CStr::from_ptr(source_tier) }.to_string_lossy();
        t.parse().unwrap_or(Tier::Active)
    };

    let tgt_tier: Tier = if target_tier.is_null() {
        Tier::Archive
    } else {
        let t = unsafe { CStr::from_ptr(target_tier) }.to_string_lossy();
        t.parse().unwrap_or(Tier::Archive)
    };

    let strategy_str = if strategy.is_null() {
        "hybrid".to_string()
    } else {
        unsafe { CStr::from_ptr(strategy) }.to_string_lossy().into_owned()
    };

    let compact_strategy = if strategy_str.starts_with("age:") {
        let secs_str = &strategy_str[4..];
        match secs_str.parse::<i64>() {
            Ok(secs) => CompactionStrategy::Age { older_than_secs: secs },
            Err(e) => {
                set_error(
                    err,
                    MastError::InvalidInput(format!("invalid age seconds: {e}")),
                );
                return ptr::null_mut();
            }
        }
    } else if strategy_str.starts_with("manual:") {
        let ids_str = &strategy_str[7..];
        let ids: Result<Vec<ulid::Ulid>, _> =
            ids_str.split(',').map(|s| s.trim().parse()).collect();
        match ids {
            Ok(ids) => CompactionStrategy::Manual { ids },
            Err(e) => {
                set_error(
                    err,
                    MastError::InvalidInput(format!("invalid manual IDs: {e}")),
                );
                return ptr::null_mut();
            }
        }
    } else if strategy_str == "access_decay" {
        CompactionStrategy::AccessDecay
    } else {
        CompactionStrategy::Hybrid
    };

    let compactor_vtable = unsafe { std::ptr::read(compactor) };
    let ffi_compactor = FfiCompactor { vtable: compactor_vtable };

    let embedder_vtable = unsafe { std::ptr::read(embedder) };
    let ffi_embedder = FfiEmbedder { vtable: embedder_vtable };

    let request = CompactRequest {
        collection: col,
        source_tier: src_tier,
        target_tier: tgt_tier,
        strategy: compact_strategy,
        max_batch_size,
        dry_run,
    };

    match h.runtime.block_on(h.mast.compact(request, &ffi_compactor, &ffi_embedder)) {
        Ok(result) => {
            let new_ids: Vec<*mut c_char> = result
                .new_memory_ids
                .iter()
                .map(|id| CString::new(id.to_string()).unwrap().into_raw())
                .collect();
            let new_ids_count = new_ids.len();
            let new_ids_ptr = if new_ids.is_empty() {
                ptr::null_mut()
            } else {
                let mut v = new_ids;
                let p = v.as_mut_ptr();
                std::mem::forget(v);
                p
            };

            let source_ids: Vec<*mut c_char> = result
                .source_memory_ids
                .iter()
                .map(|id| CString::new(id.to_string()).unwrap().into_raw())
                .collect();
            let source_ids_count = source_ids.len();
            let source_ids_ptr = if source_ids.is_empty() {
                ptr::null_mut()
            } else {
                let mut v = source_ids;
                let p = v.as_mut_ptr();
                std::mem::forget(v);
                p
            };

            Box::into_raw(Box::new(MastCompactResult {
                source_count: result.source_count,
                compacted_count: result.compacted_count,
                new_ids: new_ids_ptr,
                new_ids_count,
                source_ids: source_ids_ptr,
                source_ids_count,
            }))
        }
        Err(e) => {
            set_error(err, e);
            ptr::null_mut()
        }
    }
}

// --- Graph API (FFI) ---

fn edges_to_graph_result(edges: Vec<Edge>) -> *mut MastGraphResult {
    let count = edges.len();
    let mut entries: Vec<MastEdgeEntry> = edges
        .into_iter()
        .map(|e| {
            let id = CString::new(e.id.to_string()).unwrap();
            let source = CString::new(e.source).unwrap();
            let target = CString::new(e.target).unwrap();
            let relation = CString::new(e.relation).unwrap();
            MastEdgeEntry {
                id: id.into_raw(),
                source: source.into_raw(),
                target: target.into_raw(),
                relation: relation.into_raw(),
                weight: e.weight,
                created_at: e.created_at,
                updated_at: e.updated_at,
                valid_from: e.valid_from.unwrap_or(0),
                has_valid_from: e.valid_from.is_some(),
                valid_to: e.valid_to.unwrap_or(0),
                has_valid_to: e.valid_to.is_some(),
            }
        })
        .collect();
    let ptr = entries.as_mut_ptr();
    std::mem::forget(entries);
    Box::into_raw(Box::new(MastGraphResult {
        edges: ptr,
        count,
    }))
}

#[no_mangle]
pub extern "C" fn mast_traverse(
    handle: *mut MastHandle,
    collection: *const c_char,
    start: *const c_char,
    max_depth: usize,
    relation_filter: *const c_char,
    min_weight: f32,
    has_min_weight: bool,
    as_of: i64,
    has_as_of: bool,
    err: *mut *mut MastFfiError,
) -> *mut MastGraphResult {
    let h = unsafe { &mut *handle };
    let col = unsafe { CStr::from_ptr(collection) }.to_string_lossy().into_owned();
    let start_str = unsafe { CStr::from_ptr(start) }.to_string_lossy().into_owned();

    let rel_filter = if relation_filter.is_null() {
        None
    } else {
        Some(unsafe { CStr::from_ptr(relation_filter) }.to_string_lossy().into_owned())
    };

    let min_w = if has_min_weight { Some(min_weight) } else { None };

    let request = TraverseRequest {
        collection: col,
        start: start_str,
        max_depth,
        relation_filter: rel_filter,
        min_weight: min_w,
        as_of: has_as_of.then_some(as_of),
    };

    match h.mast.traverse(request) {
        Ok(edges) => edges_to_graph_result(edges),
        Err(e) => {
            set_error(err, e);
            ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "C" fn mast_subgraph(
    handle: *mut MastHandle,
    collection: *const c_char,
    err: *mut *mut MastFfiError,
) -> *mut MastGraphResult {
    let h = unsafe { &mut *handle };
    let col = unsafe { CStr::from_ptr(collection) }.to_string_lossy().into_owned();

    match h.mast.subgraph(&col) {
        Ok(graph) => edges_to_graph_result(graph.edges),
        Err(e) => {
            set_error(err, e);
            ptr::null_mut()
        }
    }
}

// --- Free helpers ---

#[no_mangle]
pub extern "C" fn mast_free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe { drop(CString::from_raw(s)) };
    }
}

#[no_mangle]
pub extern "C" fn mast_free_store_result(result: *mut MastStoreResult) {
    if result.is_null() {
        return;
    }
    unsafe {
        let r = Box::from_raw(result);
        if !r.id.is_null() {
            drop(CString::from_raw(r.id));
        }
    }
}

#[no_mangle]
pub extern "C" fn mast_free_recall_result(result: *mut MastRecallResult) {
    if result.is_null() {
        return;
    }
    unsafe {
        let r = Box::from_raw(result);
        let entries = Vec::from_raw_parts(r.entries, r.count, r.count);
        for entry in entries {
            if !entry.id.is_null() {
                drop(CString::from_raw(entry.id));
            }
            if !entry.content.is_null() {
                drop(CString::from_raw(entry.content));
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn mast_free_info_result(result: *mut MastInfoResult) {
    if result.is_null() {
        return;
    }
    unsafe {
        let r = Box::from_raw(result);
        let entries = Vec::from_raw_parts(r.collections, r.count, r.count);
        for entry in entries {
            if !entry.name.is_null() {
                drop(CString::from_raw(entry.name));
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn mast_free_snapshot_result(result: *mut MastSnapshotResult) {
    if !result.is_null() {
        unsafe { drop(Box::from_raw(result)) };
    }
}

#[no_mangle]
pub extern "C" fn mast_free_restore_result(result: *mut MastRestoreResult) {
    if !result.is_null() {
        unsafe { drop(Box::from_raw(result)) };
    }
}

#[no_mangle]
pub extern "C" fn mast_free_list_result(result: *mut MastListResult) {
    if result.is_null() {
        return;
    }
    unsafe {
        let r = Box::from_raw(result);
        let entries = Vec::from_raw_parts(r.entries, r.count, r.count);
        for entry in entries {
            if !entry.id.is_null() {
                drop(CString::from_raw(entry.id));
            }
            if !entry.content.is_null() {
                drop(CString::from_raw(entry.content));
            }
            if !entry.tier.is_null() {
                drop(CString::from_raw(entry.tier));
            }
            if !entry.metadata_json.is_null() {
                drop(CString::from_raw(entry.metadata_json));
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn mast_free_recall_full_result(result: *mut MastRecallFullResult) {
    if result.is_null() {
        return;
    }
    unsafe {
        let r = Box::from_raw(result);
        let entries = Vec::from_raw_parts(r.entries, r.count, r.count);
        for entry in entries {
            if !entry.id.is_null() {
                drop(CString::from_raw(entry.id));
            }
            if !entry.content.is_null() {
                drop(CString::from_raw(entry.content));
            }
            if !entry.tier.is_null() {
                drop(CString::from_raw(entry.tier));
            }
            if !entry.metadata_json.is_null() {
                drop(CString::from_raw(entry.metadata_json));
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn mast_free_compact_result(result: *mut MastCompactResult) {
    if result.is_null() {
        return;
    }
    unsafe {
        let r = Box::from_raw(result);
        if !r.new_ids.is_null() {
            let ids = Vec::from_raw_parts(r.new_ids, r.new_ids_count, r.new_ids_count);
            for id in ids {
                if !id.is_null() {
                    drop(CString::from_raw(id));
                }
            }
        }
        if !r.source_ids.is_null() {
            let ids = Vec::from_raw_parts(r.source_ids, r.source_ids_count, r.source_ids_count);
            for id in ids {
                if !id.is_null() {
                    drop(CString::from_raw(id));
                }
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn mast_free_graph_result(result: *mut MastGraphResult) {
    if result.is_null() {
        return;
    }
    unsafe {
        let r = Box::from_raw(result);
        let entries = Vec::from_raw_parts(r.edges, r.count, r.count);
        for entry in entries {
            if !entry.id.is_null() {
                drop(CString::from_raw(entry.id));
            }
            if !entry.source.is_null() {
                drop(CString::from_raw(entry.source));
            }
            if !entry.target.is_null() {
                drop(CString::from_raw(entry.target));
            }
            if !entry.relation.is_null() {
                drop(CString::from_raw(entry.relation));
            }
        }
    }
}
