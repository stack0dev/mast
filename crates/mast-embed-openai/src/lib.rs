use async_trait::async_trait;
use mast_core::embed::Embedder;
use mast_core::error::MastError;
use reqwest::Client;
use serde::{Deserialize, Serialize};

pub struct OpenAIEmbedder {
    api_key: String,
    model: String,
    dimensions: usize,
    client: Client,
    base_url: String,
}

impl OpenAIEmbedder {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            model: "text-embedding-3-small".to_string(),
            dimensions: 1536,
            client: Client::new(),
            base_url: "https://api.openai.com".to_string(),
        }
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }

    pub fn with_dimensions(mut self, dims: usize) -> Self {
        self.dimensions = dims;
        self
    }

    pub fn with_base_url(mut self, url: &str) -> Self {
        self.base_url = url.to_string();
        self
    }
}

#[derive(Serialize)]
struct EmbeddingRequest {
    model: String,
    input: Vec<String>,
    dimensions: usize,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

#[async_trait]
impl Embedder for OpenAIEmbedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, MastError> {
        let req = EmbeddingRequest {
            model: self.model.clone(),
            input: vec![text.to_string()],
            dimensions: self.dimensions,
        };

        let resp = self
            .client
            .post(&format!("{}/v1/embeddings", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&req)
            .send()
            .await
            .map_err(|e| MastError::Embedder(format!("request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp
                .text()
                .await
                .unwrap_or_else(|_| "unknown".to_string());
            return Err(MastError::Embedder(format!(
                "API error {status}: {body}"
            )));
        }

        let body: EmbeddingResponse = resp
            .json()
            .await
            .map_err(|e| MastError::Embedder(format!("failed to parse response: {e}")))?;

        body.data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .ok_or_else(|| MastError::Embedder("empty response".into()))
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, MastError> {
        let req = EmbeddingRequest {
            model: self.model.clone(),
            input: texts.to_vec(),
            dimensions: self.dimensions,
        };

        let resp = self
            .client
            .post(&format!("{}/v1/embeddings", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&req)
            .send()
            .await
            .map_err(|e| MastError::Embedder(format!("request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp
                .text()
                .await
                .unwrap_or_else(|_| "unknown".to_string());
            return Err(MastError::Embedder(format!(
                "API error {status}: {body}"
            )));
        }

        let body: EmbeddingResponse = resp
            .json()
            .await
            .map_err(|e| MastError::Embedder(format!("failed to parse response: {e}")))?;

        Ok(body.data.into_iter().map(|d| d.embedding).collect())
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn new_defaults() {
        let emb = OpenAIEmbedder::new("test-key".into());
        assert_eq!(emb.dimensions(), 1536);
        assert_eq!(emb.model, "text-embedding-3-small");
        assert_eq!(emb.api_key, "test-key");
        assert_eq!(emb.base_url, "https://api.openai.com");
    }

    #[test]
    fn with_model_overrides() {
        let emb = OpenAIEmbedder::new("k".into()).with_model("text-embedding-3-large");
        assert_eq!(emb.model, "text-embedding-3-large");
    }

    #[test]
    fn with_dimensions_overrides() {
        let emb = OpenAIEmbedder::new("k".into()).with_dimensions(768);
        assert_eq!(emb.dimensions(), 768);
    }

    #[test]
    fn with_base_url_overrides() {
        let emb = OpenAIEmbedder::new("k".into()).with_base_url("http://localhost:8080");
        assert_eq!(emb.base_url, "http://localhost:8080");
    }

    #[test]
    fn builder_chain() {
        let emb = OpenAIEmbedder::new("k".into())
            .with_model("custom-model")
            .with_dimensions(256);
        assert_eq!(emb.model, "custom-model");
        assert_eq!(emb.dimensions(), 256);
    }

    #[test]
    fn embedding_request_serializes() {
        let req = EmbeddingRequest {
            model: "test".into(),
            input: vec!["hello".into()],
            dimensions: 128,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"model\":\"test\""));
        assert!(json.contains("\"dimensions\":128"));
    }

    #[test]
    fn embedding_response_deserializes() {
        let json = r#"{"data":[{"embedding":[0.1,0.2,0.3]}]}"#;
        let resp: EmbeddingResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.data.len(), 1);
        assert_eq!(resp.data[0].embedding, vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn embedding_response_empty_data() {
        let json = r#"{"data":[]}"#;
        let resp: EmbeddingResponse = serde_json::from_str(json).unwrap();
        assert!(resp.data.is_empty());
    }

    // --- HTTP error handling tests (wiremock) ---

    fn embedder_for(server: &MockServer) -> OpenAIEmbedder {
        OpenAIEmbedder::new("test-key".into())
            .with_base_url(&server.uri())
            .with_dimensions(3)
    }

    #[tokio::test]
    async fn embed_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/embeddings"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"data":[{"embedding":[0.1,0.2,0.3]}]})),
            )
            .mount(&server)
            .await;

        let emb = embedder_for(&server);
        let vec = emb.embed("hello").await.unwrap();
        assert_eq!(vec, vec![0.1, 0.2, 0.3]);
    }

    #[tokio::test]
    async fn embed_batch_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/embeddings"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data":[
                    {"embedding":[0.1,0.2,0.3]},
                    {"embedding":[0.4,0.5,0.6]}
                ]
            })))
            .mount(&server)
            .await;

        let emb = embedder_for(&server);
        let vecs = emb
            .embed_batch(&["a".into(), "b".into()])
            .await
            .unwrap();
        assert_eq!(vecs.len(), 2);
        assert_eq!(vecs[0], vec![0.1, 0.2, 0.3]);
        assert_eq!(vecs[1], vec![0.4, 0.5, 0.6]);
    }

    #[tokio::test]
    async fn embed_401_unauthorized() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/embeddings"))
            .respond_with(
                ResponseTemplate::new(401).set_body_string("Incorrect API key provided"),
            )
            .mount(&server)
            .await;

        let emb = embedder_for(&server);
        let err = emb.embed("hello").await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("401"), "expected 401 in: {msg}");
        assert!(
            msg.contains("Incorrect API key"),
            "expected body in: {msg}"
        );
    }

    #[tokio::test]
    async fn embed_429_rate_limit() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/embeddings"))
            .respond_with(
                ResponseTemplate::new(429).set_body_string("Rate limit exceeded"),
            )
            .mount(&server)
            .await;

        let emb = embedder_for(&server);
        let err = emb.embed("hello").await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("429"), "expected 429 in: {msg}");
    }

    #[tokio::test]
    async fn embed_500_server_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/embeddings"))
            .respond_with(
                ResponseTemplate::new(500).set_body_string("Internal server error"),
            )
            .mount(&server)
            .await;

        let emb = embedder_for(&server);
        let err = emb.embed("hello").await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("500"), "expected 500 in: {msg}");
    }

    #[tokio::test]
    async fn embed_invalid_json_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/embeddings"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string("not json at all"),
            )
            .mount(&server)
            .await;

        let emb = embedder_for(&server);
        let err = emb.embed("hello").await.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("failed to parse response"),
            "expected parse error in: {msg}"
        );
    }

    #[tokio::test]
    async fn embed_empty_data_array() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/embeddings"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"data":[]})),
            )
            .mount(&server)
            .await;

        let emb = embedder_for(&server);
        let err = emb.embed("hello").await.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("empty response"),
            "expected 'empty response' in: {msg}"
        );
    }

    #[tokio::test]
    async fn embed_batch_non_2xx() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/embeddings"))
            .respond_with(ResponseTemplate::new(503).set_body_string("Service unavailable"))
            .mount(&server)
            .await;

        let emb = embedder_for(&server);
        let err = emb
            .embed_batch(&["a".into()])
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("503"), "expected 503 in: {msg}");
    }
}
