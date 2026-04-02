use async_trait::async_trait;
use mast_core::embed::Embedder;
use mast_core::error::MastError;
use reqwest::Client;
use serde::{Deserialize, Serialize};

pub struct VoyageEmbedder {
    api_key: String,
    model: String,
    dimensions: usize,
    input_type: Option<InputType>,
    client: Client,
    base_url: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InputType {
    Query,
    Document,
}

impl VoyageEmbedder {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            model: "voyage-3.5".to_string(),
            dimensions: 1024,
            input_type: None,
            client: Client::new(),
            base_url: "https://api.voyageai.com".to_string(),
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

    pub fn with_input_type(mut self, input_type: InputType) -> Self {
        self.input_type = Some(input_type);
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
    #[serde(skip_serializing_if = "Option::is_none")]
    output_dimension: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    input_type: Option<InputType>,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

impl VoyageEmbedder {
    fn build_request(&self, input: Vec<String>) -> EmbeddingRequest {
        EmbeddingRequest {
            model: self.model.clone(),
            input,
            output_dimension: Some(self.dimensions),
            input_type: self.input_type,
        }
    }

    async fn send_request(&self, req: &EmbeddingRequest) -> Result<EmbeddingResponse, MastError> {
        let resp = self
            .client
            .post(&format!("{}/v1/embeddings", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(req)
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

        resp.json()
            .await
            .map_err(|e| MastError::Embedder(format!("failed to parse response: {e}")))
    }
}

#[async_trait]
impl Embedder for VoyageEmbedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, MastError> {
        let req = self.build_request(vec![text.to_string()]);
        let body = self.send_request(&req).await?;

        body.data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .ok_or_else(|| MastError::Embedder("empty response".into()))
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, MastError> {
        let req = self.build_request(texts.to_vec());
        let body = self.send_request(&req).await?;
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
        let emb = VoyageEmbedder::new("test-key".into());
        assert_eq!(emb.dimensions(), 1024);
        assert_eq!(emb.model, "voyage-3.5");
        assert_eq!(emb.api_key, "test-key");
        assert_eq!(emb.base_url, "https://api.voyageai.com");
        assert!(emb.input_type.is_none());
    }

    #[test]
    fn with_model_overrides() {
        let emb = VoyageEmbedder::new("k".into()).with_model("voyage-3-large");
        assert_eq!(emb.model, "voyage-3-large");
    }

    #[test]
    fn with_dimensions_overrides() {
        let emb = VoyageEmbedder::new("k".into()).with_dimensions(512);
        assert_eq!(emb.dimensions(), 512);
    }

    #[test]
    fn with_input_type_sets_type() {
        let emb = VoyageEmbedder::new("k".into()).with_input_type(InputType::Query);
        assert!(matches!(emb.input_type, Some(InputType::Query)));
    }

    #[test]
    fn with_base_url_overrides() {
        let emb = VoyageEmbedder::new("k".into()).with_base_url("http://localhost:8080");
        assert_eq!(emb.base_url, "http://localhost:8080");
    }

    #[test]
    fn builder_chain() {
        let emb = VoyageEmbedder::new("k".into())
            .with_model("voyage-code-3")
            .with_dimensions(256)
            .with_input_type(InputType::Document);
        assert_eq!(emb.model, "voyage-code-3");
        assert_eq!(emb.dimensions(), 256);
        assert!(matches!(emb.input_type, Some(InputType::Document)));
    }

    #[test]
    fn request_serializes_with_all_fields() {
        let req = EmbeddingRequest {
            model: "voyage-3.5".into(),
            input: vec!["hello".into()],
            output_dimension: Some(1024),
            input_type: Some(InputType::Query),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"model\":\"voyage-3.5\""));
        assert!(json.contains("\"output_dimension\":1024"));
        assert!(json.contains("\"input_type\":\"query\""));
    }

    #[test]
    fn request_omits_none_fields() {
        let req = EmbeddingRequest {
            model: "voyage-3.5".into(),
            input: vec!["hello".into()],
            output_dimension: None,
            input_type: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(!json.contains("output_dimension"));
        assert!(!json.contains("input_type"));
    }

    #[test]
    fn response_deserializes() {
        let json = r#"{"object":"list","data":[{"object":"embedding","embedding":[0.1,0.2,0.3],"index":0}],"model":"voyage-3.5","usage":{"total_tokens":5}}"#;
        let resp: EmbeddingResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.data.len(), 1);
        assert_eq!(resp.data[0].embedding, vec![0.1, 0.2, 0.3]);
    }

    fn embedder_for(server: &MockServer) -> VoyageEmbedder {
        VoyageEmbedder::new("test-key".into())
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
        assert!(msg.contains("empty response"), "expected 'empty response' in: {msg}");
    }

    #[tokio::test]
    async fn embed_invalid_json_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/embeddings"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string("not json"),
            )
            .mount(&server)
            .await;

        let emb = embedder_for(&server);
        let err = emb.embed("hello").await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("failed to parse response"), "expected parse error in: {msg}");
    }
}
