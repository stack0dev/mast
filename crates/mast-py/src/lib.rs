use mast_core::config::MastConfig;
use mast_core::error::MastError;
use mast_core::types::*;
use mast_core::Mast;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

fn to_pyerr(e: MastError) -> PyErr {
    PyRuntimeError::new_err(e.to_string())
}

struct PyEmbedder {
    obj: Py<PyAny>,
}

unsafe impl Send for PyEmbedder {}
unsafe impl Sync for PyEmbedder {}

#[async_trait::async_trait]
impl mast_core::embed::Embedder for PyEmbedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, MastError> {
        Python::with_gil(|py| {
            let result = self
                .obj
                .call_method1(py, "embed", (text,))
                .map_err(|e| MastError::Embedder(format!("Python embed error: {e}")))?;
            let vec: Vec<f32> = result
                .extract(py)
                .map_err(|e| MastError::Embedder(format!("failed to extract Vec<f32>: {e}")))?;
            Ok(vec)
        })
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, MastError> {
        Python::with_gil(|py| {
            let py_texts: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
            let has_method = self
                .obj
                .getattr(py, "embed_batch")
                .is_ok();

            if has_method {
                let result = self
                    .obj
                    .call_method1(py, "embed_batch", (py_texts,))
                    .map_err(|e| MastError::Embedder(format!("Python embed_batch error: {e}")))?;
                let vecs: Vec<Vec<f32>> = result
                    .extract(py)
                    .map_err(|e| {
                        MastError::Embedder(format!("failed to extract Vec<Vec<f32>>: {e}"))
                    })?;
                Ok(vecs)
            } else {
                // Fall back to sequential
                let mut results = Vec::with_capacity(texts.len());
                for text in texts {
                    let result = self
                        .obj
                        .call_method1(py, "embed", (text.as_str(),))
                        .map_err(|e| MastError::Embedder(format!("Python embed error: {e}")))?;
                    let vec: Vec<f32> = result
                        .extract(py)
                        .map_err(|e| {
                            MastError::Embedder(format!("failed to extract Vec<f32>: {e}"))
                        })?;
                    results.push(vec);
                }
                Ok(results)
            }
        })
    }

    fn dimensions(&self) -> usize {
        Python::with_gil(|py| {
            self.obj
                .call_method0(py, "dimensions")
                .and_then(|r| r.extract(py))
                .unwrap_or(0)
        })
    }
}

#[pyclass]
pub struct PyMemory {
    #[pyo3(get)]
    pub id: String,
    #[pyo3(get)]
    pub collection: String,
    #[pyo3(get)]
    pub content: String,
    #[pyo3(get)]
    pub metadata: HashMap<String, String>,
    #[pyo3(get)]
    pub tier: String,
    #[pyo3(get)]
    pub created_at: i64,
    #[pyo3(get)]
    pub access_count: u64,
    #[pyo3(get)]
    pub last_accessed: i64,
}

impl From<Memory> for PyMemory {
    fn from(m: Memory) -> Self {
        PyMemory {
            id: m.id.to_string(),
            collection: m.collection,
            content: m.content,
            metadata: m.metadata,
            tier: m.tier.to_string(),
            created_at: m.created_at,
            access_count: m.access_count,
            last_accessed: m.last_accessed,
        }
    }
}

#[pyclass]
pub struct PyRecallResult {
    #[pyo3(get)]
    pub memory: Py<PyMemory>,
    #[pyo3(get)]
    pub score: f32,
}

#[pyclass]
pub struct PyCollectionInfo {
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub dimensions: usize,
    #[pyo3(get)]
    pub memory_count: u64,
}

#[pyclass]
pub struct PyEdge {
    #[pyo3(get)]
    pub id: String,
    #[pyo3(get)]
    pub source: String,
    #[pyo3(get)]
    pub target: String,
    #[pyo3(get)]
    pub relation: String,
    #[pyo3(get)]
    pub weight: f32,
    #[pyo3(get)]
    pub valid_from: Option<i64>,
    #[pyo3(get)]
    pub valid_to: Option<i64>,
}

impl From<Edge> for PyEdge {
    fn from(e: Edge) -> Self {
        PyEdge {
            id: e.id.to_string(),
            source: e.source,
            target: e.target,
            relation: e.relation,
            weight: e.weight,
            valid_from: e.valid_from,
            valid_to: e.valid_to,
        }
    }
}

#[pyclass]
pub struct PySnapshotResult {
    #[pyo3(get)]
    pub collection: String,
    #[pyo3(get)]
    pub memory_count: u64,
    #[pyo3(get)]
    pub edge_count: u64,
}

#[pyclass]
pub struct PyRestoreResult {
    #[pyo3(get)]
    pub collection: String,
    #[pyo3(get)]
    pub memory_count: u64,
    #[pyo3(get)]
    pub edge_count: u64,
    #[pyo3(get)]
    pub skipped_count: u64,
    #[pyo3(get)]
    pub warnings: Vec<String>,
}

#[pyclass(name = "Mast")]
pub struct MastPy {
    inner: Arc<Mutex<Mast>>,
    runtime: Arc<tokio::runtime::Runtime>,
}

#[pymethods]
impl MastPy {
    #[new]
    fn new(path: &str) -> PyResult<Self> {
        let config = MastConfig::default().with_db_path(path);
        let mast = Mast::open(config).map_err(to_pyerr)?;
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| PyRuntimeError::new_err(format!("failed to create runtime: {e}")))?;
        Ok(MastPy {
            inner: Arc::new(Mutex::new(mast)),
            runtime: Arc::new(runtime),
        })
    }

    #[pyo3(signature = (collection, content, embedder, metadata=None, tier=None, ttl=None, created_at=None))]
    fn store<'py>(
        &self,
        py: Python<'py>,
        collection: &str,
        content: &str,
        embedder: Py<PyAny>,
        metadata: Option<HashMap<String, String>>,
        tier: Option<&str>,
        ttl: Option<i64>,
        created_at: Option<i64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let emb = PyEmbedder { obj: embedder };
        let tier_val = tier
            .map(|t| t.parse::<Tier>().unwrap_or(Tier::Active))
            .unwrap_or(Tier::Active);

        let request = StoreRequest {
            collection: collection.to_string(),
            content: content.to_string(),
            embedding: None,
            metadata: metadata.unwrap_or_default(),
            tier: tier_val,
            ttl,
            created_at,
        };

        let inner = self.inner.clone();
        let runtime = self.runtime.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let memory = {
                let mut mast = inner.lock().map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
                runtime
                    .block_on(mast.store(request, &emb))
                    .map_err(to_pyerr)?
            };
            Python::with_gil(|py| {
                let obj = Py::new(py, PyMemory::from(memory))?;
                Ok(obj)
            })
        })
    }

    #[pyo3(signature = (collection, embedder, query=None, text_query=None, search_mode=None, limit=None, filter=None, reference_time=None))]
    fn recall<'py>(
        &self,
        py: Python<'py>,
        collection: &str,
        embedder: Py<PyAny>,
        query: Option<&str>,
        text_query: Option<&str>,
        search_mode: Option<&str>,
        limit: Option<usize>,
        filter: Option<HashMap<String, String>>,
        reference_time: Option<i64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let emb = PyEmbedder { obj: embedder };
        let mode: SearchMode = search_mode
            .map(|s| s.parse().unwrap_or(SearchMode::Vector))
            .unwrap_or(SearchMode::Vector);

        let metadata_filter = filter.map(|f| {
            let filters: Vec<MetadataFilter> = f
                .into_iter()
                .map(|(k, v)| MetadataFilter::Eq(k, v))
                .collect();
            if filters.len() == 1 {
                filters.into_iter().next().unwrap()
            } else {
                MetadataFilter::And(filters)
            }
        });

        let request = RecallRequest {
            collection: collection.to_string(),
            query: query.map(|s| s.to_string()),
            query_embedding: None,
            text_query: text_query.map(|s| s.to_string()),
            search_mode: mode,
            limit: limit.unwrap_or(10),
            filter: metadata_filter,
            tier_weights: None,
            time_decay: None,
            reference_time,
            time_anchor: None,
        };

        let inner = self.inner.clone();
        let runtime = self.runtime.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let results = {
                let mut mast = inner.lock().map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
                runtime
                    .block_on(mast.recall(request, &emb))
                    .map_err(to_pyerr)?
            };

            Python::with_gil(|py| {
                let py_results: Vec<PyRecallResult> = results
                    .into_iter()
                    .map(|r| {
                        let mem = Py::new(py, PyMemory::from(r.memory))?;
                        Ok(PyRecallResult {
                            memory: mem,
                            score: r.score,
                        })
                    })
                    .collect::<PyResult<Vec<_>>>()?;
                Ok(py_results)
            })
        })
    }

    fn get(&self, collection: &str, id: &str) -> PyResult<Option<PyMemory>> {
        let ulid: ulid::Ulid = id
            .parse()
            .map_err(|e| PyRuntimeError::new_err(format!("invalid ULID: {e}")))?;
        let mast = self.inner.lock().map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        let memory = mast.get(collection, &ulid).map_err(to_pyerr)?;
        Ok(memory.map(PyMemory::from))
    }

    fn delete(&self, collection: &str, id: &str) -> PyResult<()> {
        let ulid: ulid::Ulid = id
            .parse()
            .map_err(|e| PyRuntimeError::new_err(format!("invalid ULID: {e}")))?;
        let mut mast = self.inner.lock().map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        mast.delete(collection, &ulid).map_err(to_pyerr)
    }

    fn list(&self, _py: Python<'_>, collection: &str) -> PyResult<Vec<PyMemory>> {
        let mast = self.inner.lock().map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        let memories = mast.list(collection).map_err(to_pyerr)?;
        Ok(memories.into_iter().map(PyMemory::from).collect())
    }

    fn info(&self) -> PyResult<Vec<PyCollectionInfo>> {
        let mast = self.inner.lock().map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        let collections = mast.info().map_err(to_pyerr)?;
        Ok(collections
            .into_iter()
            .map(|c| PyCollectionInfo {
                name: c.name,
                dimensions: c.dimensions,
                memory_count: c.memory_count,
            })
            .collect())
    }

    fn vacuum(&self, collection: &str) -> PyResult<usize> {
        let mut mast = self.inner.lock().map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        let result = mast.vacuum(collection).map_err(to_pyerr)?;
        Ok(result.expired_count)
    }

    #[pyo3(signature = (collection, source, target, relation, weight=None, valid_from=None, valid_to=None))]
    fn relate(
        &self,
        collection: &str,
        source: &str,
        target: &str,
        relation: &str,
        weight: Option<f32>,
        valid_from: Option<i64>,
        valid_to: Option<i64>,
    ) -> PyResult<String> {
        let mast = self.inner.lock().map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        let request = RelateRequest {
            collection: collection.to_string(),
            source: source.to_string(),
            target: target.to_string(),
            relation: relation.to_string(),
            weight: weight.unwrap_or(1.0),
            memory_ids: vec![],
            valid_from,
            valid_to,
        };
        let id = mast.relate(request).map_err(to_pyerr)?;
        Ok(id.to_string())
    }

    fn unrelate(
        &self,
        collection: &str,
        source: &str,
        target: &str,
        relation: &str,
    ) -> PyResult<()> {
        let mast = self.inner.lock().map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        mast.unrelate(collection, source, target, relation)
            .map_err(to_pyerr)
    }

    /// Set `valid_to` on the open edges for this fact. Returns how many were closed.
    #[pyo3(signature = (collection, source, target, relation, ended_at=None))]
    fn invalidate(
        &self,
        collection: &str,
        source: &str,
        target: &str,
        relation: &str,
        ended_at: Option<i64>,
    ) -> PyResult<usize> {
        let mast = self.inner.lock().map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        mast.invalidate(InvalidateRequest {
            collection: collection.to_string(),
            source: source.to_string(),
            target: target.to_string(),
            relation: relation.to_string(),
            ended_at,
        })
        .map_err(to_pyerr)
    }

    #[pyo3(signature = (collection, start, max_depth=None, relation=None, min_weight=None, as_of=None))]
    fn traverse(
        &self,
        _py: Python<'_>,
        collection: &str,
        start: &str,
        max_depth: Option<usize>,
        relation: Option<&str>,
        min_weight: Option<f32>,
        as_of: Option<i64>,
    ) -> PyResult<Vec<PyEdge>> {
        let mast = self.inner.lock().map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        let request = TraverseRequest {
            collection: collection.to_string(),
            start: start.to_string(),
            max_depth: max_depth.unwrap_or(3),
            relation_filter: relation.map(|s| s.to_string()),
            min_weight,
            as_of,
        };
        let edges = mast.traverse(request).map_err(to_pyerr)?;
        Ok(edges.into_iter().map(PyEdge::from).collect())
    }

    fn snapshot(&self, collection: &str, output: &str) -> PyResult<PySnapshotResult> {
        let mast = self.inner.lock().map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        let mut file = std::fs::File::create(output)
            .map_err(|e| PyRuntimeError::new_err(format!("failed to create file: {e}")))?;
        let result = mast.snapshot(collection, &mut file).map_err(to_pyerr)?;
        Ok(PySnapshotResult {
            collection: result.collection,
            memory_count: result.memory_count,
            edge_count: result.edge_count,
        })
    }

    #[pyo3(signature = (input, merge=None))]
    fn restore(&self, input: &str, merge: Option<bool>) -> PyResult<PyRestoreResult> {
        let mut mast = self.inner.lock().map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        let file = std::fs::File::open(input)
            .map_err(|e| PyRuntimeError::new_err(format!("failed to open file: {e}")))?;
        let mut reader = std::io::BufReader::new(file);
        let result = mast.restore(&mut reader, merge.unwrap_or(false)).map_err(to_pyerr)?;
        Ok(PyRestoreResult {
            collection: result.collection,
            memory_count: result.memory_count,
            edge_count: result.edge_count,
            skipped_count: result.skipped_count,
            warnings: result.warnings,
        })
    }

    fn close(&self) -> PyResult<()> {
        let mut mast = self.inner.lock().map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        mast.close().map_err(to_pyerr)
    }
}

#[pymodule]
fn mast(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<MastPy>()?;
    m.add_class::<PyMemory>()?;
    m.add_class::<PyRecallResult>()?;
    m.add_class::<PyCollectionInfo>()?;
    m.add_class::<PyEdge>()?;
    m.add_class::<PySnapshotResult>()?;
    m.add_class::<PyRestoreResult>()?;
    Ok(())
}
