use mast_core::config::MastConfig;
use mast_core::error::MastError;
use mast_core::types::*;
use mast_core::Mast;
use napi::bindgen_prelude::AsyncTask;
use napi::threadsafe_function::{ErrorStrategy, ThreadsafeFunction};
use napi_derive::napi;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

fn to_napi_err(e: MastError) -> napi::Error {
    napi::Error::from_reason(e.to_string())
}

struct JsEmbedder {
    embed_fn: ThreadsafeFunction<String, ErrorStrategy::Fatal>,
    dims: usize,
}

unsafe impl Send for JsEmbedder {}
unsafe impl Sync for JsEmbedder {}

#[async_trait::async_trait]
impl mast_core::embed::Embedder for JsEmbedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, MastError> {
        let text = text.to_string();
        let result: Vec<f64> = self
            .embed_fn
            .call_async(text)
            .await
            .map_err(|e| MastError::Embedder(format!("JS embed error: {e}")))?;
        Ok(result.into_iter().map(|v| v as f32).collect())
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, MastError> {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed(text).await?);
        }
        Ok(results)
    }

    fn dimensions(&self) -> usize {
        self.dims
    }
}

#[napi(object)]
pub struct JsMemory {
    pub id: String,
    pub collection: String,
    pub content: String,
    pub metadata: HashMap<String, String>,
    pub tier: String,
    pub created_at: i64,
    pub access_count: i64,
    pub last_accessed: i64,
}

impl From<Memory> for JsMemory {
    fn from(m: Memory) -> Self {
        JsMemory {
            id: m.id.to_string(),
            collection: m.collection,
            content: m.content,
            metadata: m.metadata,
            tier: m.tier.to_string(),
            created_at: m.created_at,
            access_count: m.access_count as i64,
            last_accessed: m.last_accessed,
        }
    }
}

#[napi(object)]
pub struct JsRecallResult {
    pub memory: JsMemory,
    pub score: f64,
}

#[napi(object)]
pub struct JsCollectionInfo {
    pub name: String,
    pub dimensions: i64,
    pub memory_count: i64,
}

#[napi(object)]
pub struct JsEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub relation: String,
    pub weight: f64,
}

impl From<Edge> for JsEdge {
    fn from(e: Edge) -> Self {
        JsEdge {
            id: e.id.to_string(),
            source: e.source,
            target: e.target,
            relation: e.relation,
            weight: e.weight as f64,
        }
    }
}

#[napi(object)]
pub struct JsSnapshotResult {
    pub collection: String,
    pub memory_count: i64,
    pub edge_count: i64,
}

#[napi(object)]
pub struct JsRestoreResult {
    pub collection: String,
    pub memory_count: i64,
    pub edge_count: i64,
    pub skipped_count: i64,
    pub warnings: Vec<String>,
}

#[napi(object)]
pub struct JsBatchItem {
    pub content: String,
    pub embedding: Option<Vec<f64>>,
    pub metadata: Option<HashMap<String, String>>,
    pub tier: Option<String>,
    pub ttl: Option<i64>,
}

#[napi]
pub struct MastDb {
    inner: Arc<Mutex<Mast>>,
    runtime: Arc<tokio::runtime::Runtime>,
    #[cfg(feature = "local-embedder")]
    local_embedder: Mutex<Option<mast_embed_local::LocalEmbedder>>,
}

#[napi]
impl MastDb {
    #[napi(constructor)]
    pub fn new(path: String, durability: Option<String>) -> napi::Result<Self> {
        let dur = match durability.as_deref() {
            Some("eventual") => mast_core::config::Durability::Eventual,
            _ => mast_core::config::Durability::Immediate,
        };
        let config = MastConfig::default().with_db_path(&path).with_durability(dur);
        let mast = Mast::open(config).map_err(to_napi_err)?;
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| napi::Error::from_reason(format!("runtime error: {e}")))?;
        Ok(MastDb {
            inner: Arc::new(Mutex::new(mast)),
            runtime: Arc::new(runtime),
            #[cfg(feature = "local-embedder")]
            local_embedder: Mutex::new(None),
        })
    }

    #[napi]
    pub fn store(
        &self,
        collection: String,
        content: String,
        embed_fn: ThreadsafeFunction<String, ErrorStrategy::Fatal>,
        dimensions: i64,
        metadata: Option<HashMap<String, String>>,
        tier: Option<String>,
        ttl: Option<i64>,
        embedding: Option<Vec<f64>>,
    ) -> napi::Result<JsMemory> {
        let embedder = JsEmbedder {
            embed_fn,
            dims: dimensions as usize,
        };
        let tier_val = tier
            .map(|t| t.parse::<Tier>().unwrap_or(Tier::Active))
            .unwrap_or(Tier::Active);

        let pre_embedding = embedding.map(|e| e.into_iter().map(|v| v as f32).collect());

        let request = StoreRequest {
            collection,
            content,
            embedding: pre_embedding,
            metadata: metadata.unwrap_or_default(),
            tier: tier_val,
            ttl,
        };

        let mut mast = self
            .inner
            .lock()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let memory = self
            .runtime
            .block_on(mast.store(request, &embedder))
            .map_err(to_napi_err)?;
        Ok(JsMemory::from(memory))
    }

    #[napi]
    pub fn store_batch(
        &self,
        collection: String,
        items: Vec<JsBatchItem>,
        embed_fn: ThreadsafeFunction<String, ErrorStrategy::Fatal>,
        dimensions: i64,
    ) -> napi::Result<Vec<JsMemory>> {
        let embedder = JsEmbedder {
            embed_fn,
            dims: dimensions as usize,
        };

        let batch_items: Vec<BatchStoreItem> = items
            .into_iter()
            .map(|item| {
                let embedding =
                    item.embedding
                        .map(|e| e.into_iter().map(|v| v as f32).collect());
                let tier = item
                    .tier
                    .map(|t| t.parse::<Tier>().unwrap_or(Tier::Active))
                    .unwrap_or(Tier::Active);
                BatchStoreItem {
                    content: item.content,
                    embedding,
                    metadata: item.metadata.unwrap_or_default(),
                    tier,
                    ttl: item.ttl,
                }
            })
            .collect();

        let request = BatchStoreRequest {
            collection,
            items: batch_items,
        };

        let mut mast = self
            .inner
            .lock()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let memories = self
            .runtime
            .block_on(mast.store_batch(request, &embedder))
            .map_err(to_napi_err)?;
        Ok(memories.into_iter().map(JsMemory::from).collect())
    }

    #[napi]
    pub fn recall(
        &self,
        collection: String,
        embed_fn: ThreadsafeFunction<String, ErrorStrategy::Fatal>,
        dimensions: i64,
        query: Option<String>,
        text_query: Option<String>,
        search_mode: Option<String>,
        limit: Option<i64>,
        query_embedding: Option<Vec<f64>>,
    ) -> napi::Result<Vec<JsRecallResult>> {
        let embedder = JsEmbedder {
            embed_fn,
            dims: dimensions as usize,
        };
        let mode: SearchMode = search_mode
            .map(|s| s.parse().unwrap_or(SearchMode::Vector))
            .unwrap_or(SearchMode::Vector);

        let pre_embedding =
            query_embedding.map(|e| e.into_iter().map(|v| v as f32).collect());

        let request = RecallRequest {
            collection,
            query,
            query_embedding: pre_embedding,
            text_query,
            search_mode: mode,
            limit: limit.unwrap_or(10) as usize,
            filter: None,
            tier_weights: None,
            time_decay: None,
        };

        let mut mast = self
            .inner
            .lock()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let results = self
            .runtime
            .block_on(mast.recall(request, &embedder))
            .map_err(to_napi_err)?;

        Ok(results
            .into_iter()
            .map(|r| JsRecallResult {
                memory: JsMemory::from(r.memory),
                score: r.score as f64,
            })
            .collect())
    }

    #[napi]
    pub fn get(&self, collection: String, id: String) -> napi::Result<Option<JsMemory>> {
        let ulid: ulid::Ulid = id
            .parse()
            .map_err(|e| napi::Error::from_reason(format!("invalid ULID: {e}")))?;
        let mast = self
            .inner
            .lock()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let memory = mast.get(&collection, &ulid).map_err(to_napi_err)?;
        Ok(memory.map(JsMemory::from))
    }

    #[napi]
    pub fn delete(&self, collection: String, id: String) -> napi::Result<()> {
        let ulid: ulid::Ulid = id
            .parse()
            .map_err(|e| napi::Error::from_reason(format!("invalid ULID: {e}")))?;
        let mut mast = self
            .inner
            .lock()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        mast.delete(&collection, &ulid).map_err(to_napi_err)
    }

    #[napi]
    pub fn list(&self, collection: String) -> napi::Result<Vec<JsMemory>> {
        let mast = self
            .inner
            .lock()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let memories = mast.list(&collection).map_err(to_napi_err)?;
        Ok(memories.into_iter().map(JsMemory::from).collect())
    }

    #[napi]
    pub fn info(&self) -> napi::Result<Vec<JsCollectionInfo>> {
        let mast = self
            .inner
            .lock()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let collections = mast.info().map_err(to_napi_err)?;
        Ok(collections
            .into_iter()
            .map(|c| JsCollectionInfo {
                name: c.name,
                dimensions: c.dimensions as i64,
                memory_count: c.memory_count as i64,
            })
            .collect())
    }

    #[napi]
    pub fn vacuum(&self, collection: String) -> napi::Result<i64> {
        let mut mast = self
            .inner
            .lock()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let result = mast.vacuum(&collection).map_err(to_napi_err)?;
        Ok(result.expired_count as i64)
    }

    #[napi]
    pub fn relate(
        &self,
        collection: String,
        source: String,
        target: String,
        relation: String,
        weight: Option<f64>,
    ) -> napi::Result<String> {
        let mast = self
            .inner
            .lock()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let request = RelateRequest {
            collection,
            source,
            target,
            relation,
            weight: weight.unwrap_or(1.0) as f32,
            memory_ids: vec![],
        };
        let id = mast.relate(request).map_err(to_napi_err)?;
        Ok(id.to_string())
    }

    #[napi]
    pub fn unrelate(
        &self,
        collection: String,
        source: String,
        target: String,
        relation: String,
    ) -> napi::Result<()> {
        let mast = self
            .inner
            .lock()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        mast.unrelate(&collection, &source, &target, &relation)
            .map_err(to_napi_err)
    }

    #[napi]
    pub fn traverse(
        &self,
        collection: String,
        start: String,
        max_depth: Option<i64>,
        relation: Option<String>,
        min_weight: Option<f64>,
    ) -> napi::Result<Vec<JsEdge>> {
        let mast = self
            .inner
            .lock()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let request = TraverseRequest {
            collection,
            start,
            max_depth: max_depth.unwrap_or(3) as usize,
            relation_filter: relation,
            min_weight: min_weight.map(|w| w as f32),
        };
        let edges = mast.traverse(request).map_err(to_napi_err)?;
        Ok(edges.into_iter().map(JsEdge::from).collect())
    }

    #[napi]
    pub fn subgraph(&self, collection: String) -> napi::Result<Vec<JsEdge>> {
        let mast = self
            .inner
            .lock()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let graph = mast.subgraph(&collection).map_err(to_napi_err)?;
        Ok(graph.edges.into_iter().map(JsEdge::from).collect())
    }

    #[napi]
    pub fn snapshot(&self, collection: String, output: String) -> napi::Result<JsSnapshotResult> {
        let mast = self
            .inner
            .lock()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let mut file = std::fs::File::create(&output)
            .map_err(|e| napi::Error::from_reason(format!("failed to create file: {e}")))?;
        let result = mast.snapshot(&collection, &mut file).map_err(to_napi_err)?;
        Ok(JsSnapshotResult {
            collection: result.collection,
            memory_count: result.memory_count as i64,
            edge_count: result.edge_count as i64,
        })
    }

    #[napi]
    pub fn restore(&self, input: String, merge: Option<bool>) -> napi::Result<JsRestoreResult> {
        let mut mast = self
            .inner
            .lock()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let file = std::fs::File::open(&input)
            .map_err(|e| napi::Error::from_reason(format!("failed to open file: {e}")))?;
        let mut reader = std::io::BufReader::new(file);
        let result = mast.restore(&mut reader, merge.unwrap_or(false)).map_err(to_napi_err)?;
        Ok(JsRestoreResult {
            collection: result.collection,
            memory_count: result.memory_count as i64,
            edge_count: result.edge_count as i64,
            skipped_count: result.skipped_count as i64,
            warnings: result.warnings,
        })
    }

    #[napi]
    pub fn close(&self) -> napi::Result<()> {
        let mut mast = self
            .inner
            .lock()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        mast.close().map_err(to_napi_err)
    }

    #[napi]
    pub fn store_async(
        &self,
        collection: String,
        content: String,
        embed_fn: ThreadsafeFunction<String, ErrorStrategy::Fatal>,
        dimensions: i64,
        metadata: Option<HashMap<String, String>>,
        tier: Option<String>,
        ttl: Option<i64>,
    ) -> napi::Result<AsyncTask<StoreTask>> {
        let embedder = JsEmbedder {
            embed_fn,
            dims: dimensions as usize,
        };
        let tier_val = tier
            .map(|t| t.parse::<Tier>().unwrap_or(Tier::Active))
            .unwrap_or(Tier::Active);

        let request = StoreRequest {
            collection,
            content,
            embedding: None,
            metadata: metadata.unwrap_or_default(),
            tier: tier_val,
            ttl,
        };

        Ok(AsyncTask::new(StoreTask {
            inner: self.inner.clone(),
            runtime: self.runtime.clone(),
            request,
            embedder,
        }))
    }

    #[napi]
    pub fn recall_async(
        &self,
        collection: String,
        embed_fn: ThreadsafeFunction<String, ErrorStrategy::Fatal>,
        dimensions: i64,
        query: Option<String>,
        text_query: Option<String>,
        search_mode: Option<String>,
        limit: Option<i64>,
    ) -> napi::Result<AsyncTask<RecallTask>> {
        let embedder = JsEmbedder {
            embed_fn,
            dims: dimensions as usize,
        };
        let mode: SearchMode = search_mode
            .map(|s| s.parse().unwrap_or(SearchMode::Vector))
            .unwrap_or(SearchMode::Vector);

        let request = RecallRequest {
            collection,
            query,
            query_embedding: None,
            text_query,
            search_mode: mode,
            limit: limit.unwrap_or(10) as usize,
            filter: None,
            tier_weights: None,
            time_decay: None,
        };

        Ok(AsyncTask::new(RecallTask {
            inner: self.inner.clone(),
            runtime: self.runtime.clone(),
            request,
            embedder,
        }))
    }
}

#[cfg(feature = "local-embedder")]
#[napi]
impl MastDb {
    fn get_or_init_local_embedder(
        &self,
    ) -> napi::Result<std::sync::MutexGuard<'_, Option<mast_embed_local::LocalEmbedder>>> {
        let mut guard = self
            .local_embedder
            .lock()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        if guard.is_none() {
            let embedder = mast_embed_local::LocalEmbedder::new().map_err(to_napi_err)?;
            *guard = Some(embedder);
        }
        Ok(guard)
    }

    #[napi]
    pub fn store_local(
        &self,
        collection: String,
        content: String,
        metadata: Option<HashMap<String, String>>,
        tier: Option<String>,
        ttl: Option<i64>,
    ) -> napi::Result<JsMemory> {
        let embedder_guard = self.get_or_init_local_embedder()?;
        let embedder = embedder_guard.as_ref().unwrap();
        let tier_val = tier
            .map(|t| t.parse::<Tier>().unwrap_or(Tier::Active))
            .unwrap_or(Tier::Active);

        let request = StoreRequest {
            collection,
            content,
            embedding: None,
            metadata: metadata.unwrap_or_default(),
            tier: tier_val,
            ttl,
        };

        let mut mast = self
            .inner
            .lock()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let memory = self
            .runtime
            .block_on(mast.store(request, embedder))
            .map_err(to_napi_err)?;
        Ok(JsMemory::from(memory))
    }

    #[napi]
    pub fn recall_local(
        &self,
        collection: String,
        query: Option<String>,
        text_query: Option<String>,
        search_mode: Option<String>,
        limit: Option<i64>,
    ) -> napi::Result<Vec<JsRecallResult>> {
        let embedder_guard = self.get_or_init_local_embedder()?;
        let embedder = embedder_guard.as_ref().unwrap();
        let mode: SearchMode = search_mode
            .map(|s| s.parse().unwrap_or(SearchMode::Vector))
            .unwrap_or(SearchMode::Vector);

        let request = RecallRequest {
            collection,
            query,
            query_embedding: None,
            text_query,
            search_mode: mode,
            limit: limit.unwrap_or(10) as usize,
            filter: None,
            tier_weights: None,
            time_decay: None,
        };

        let mut mast = self
            .inner
            .lock()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let results = self
            .runtime
            .block_on(mast.recall(request, embedder))
            .map_err(to_napi_err)?;

        Ok(results
            .into_iter()
            .map(|r| JsRecallResult {
                memory: JsMemory::from(r.memory),
                score: r.score as f64,
            })
            .collect())
    }
}

pub struct StoreTask {
    inner: Arc<Mutex<Mast>>,
    runtime: Arc<tokio::runtime::Runtime>,
    request: StoreRequest,
    embedder: JsEmbedder,
}

impl napi::Task for StoreTask {
    type Output = JsMemory;
    type JsValue = JsMemory;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        let mut mast = self
            .inner
            .lock()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let request = std::mem::replace(
            &mut self.request,
            StoreRequest {
                collection: String::new(),
                content: String::new(),
                embedding: None,
                metadata: HashMap::new(),
                tier: Tier::Active,
                ttl: None,
            },
        );
        let memory = self
            .runtime
            .block_on(mast.store(request, &self.embedder))
            .map_err(to_napi_err)?;
        Ok(JsMemory::from(memory))
    }

    fn resolve(&mut self, _env: napi::Env, output: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(output)
    }
}

pub struct RecallTask {
    inner: Arc<Mutex<Mast>>,
    runtime: Arc<tokio::runtime::Runtime>,
    request: RecallRequest,
    embedder: JsEmbedder,
}

impl napi::Task for RecallTask {
    type Output = Vec<JsRecallResult>;
    type JsValue = Vec<JsRecallResult>;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        let mut mast = self
            .inner
            .lock()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let request = std::mem::replace(
            &mut self.request,
            RecallRequest {
                collection: String::new(),
                query: None,
                query_embedding: None,
                text_query: None,
                search_mode: SearchMode::Vector,
                limit: 10,
                filter: None,
                tier_weights: None,
                time_decay: None,
            },
        );
        let results = self
            .runtime
            .block_on(mast.recall(request, &self.embedder))
            .map_err(to_napi_err)?;
        Ok(results
            .into_iter()
            .map(|r| JsRecallResult {
                memory: JsMemory::from(r.memory),
                score: r.score as f64,
            })
            .collect())
    }

    fn resolve(&mut self, _env: napi::Env, output: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(output)
    }
}
