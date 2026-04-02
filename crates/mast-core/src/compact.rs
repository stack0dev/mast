use crate::error::MastError;
use crate::types::Memory;
use async_trait::async_trait;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

#[derive(Debug)]
pub struct CompactedMemory {
    pub content: String,
    pub metadata: HashMap<String, String>,
}

#[async_trait]
pub trait Compactor: Send + Sync {
    async fn synthesize(&self, memories: &[Memory]) -> Result<CompactedMemory, MastError>;
}

/// Test compactor that concatenates memory content with " | ".
pub struct MockCompactor;

#[async_trait]
impl Compactor for MockCompactor {
    async fn synthesize(&self, memories: &[Memory]) -> Result<CompactedMemory, MastError> {
        let content = memories
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join(" | ");
        Ok(CompactedMemory {
            content,
            metadata: HashMap::new(),
        })
    }
}

/// Async LLM callback: takes a prompt string, returns the LLM response or an error.
pub type LlmFn = Box<
    dyn Fn(&str) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> + Send + Sync,
>;

/// Compactor that delegates synthesis to an LLM via a user-provided callback.
///
/// The LLM is prompted to return JSON `{"content": "...", "metadata": {...}}`.
/// If the response isn't valid JSON, markdown code fences are stripped and retried.
/// As a final fallback the raw response becomes the content with empty metadata.
pub struct LlmCompactor {
    llm: LlmFn,
}

#[derive(serde::Deserialize)]
struct CompactResponse {
    content: String,
    metadata: Option<HashMap<String, String>>,
}

impl LlmCompactor {
    pub fn new(llm: LlmFn) -> Self {
        Self { llm }
    }

    fn build_prompt(memories: &[Memory]) -> String {
        let mut prompt = format!(
            "You are a memory compaction system. Synthesize these {} memories into a single \
             concise summary that preserves all important information.\n\nMemories:",
            memories.len()
        );
        for (i, m) in memories.iter().enumerate() {
            prompt.push_str(&format!("\n{}. {}", i + 1, m.content));
        }
        prompt.push_str(
            "\n\nRespond with JSON only:\n\
             {\"content\": \"your synthesized summary\", \"metadata\": {\"key\": \"value\"}}\n\n\
             The \"content\" field should be a concise synthesis preserving key facts and \
             relationships. The \"metadata\" field should contain any shared attributes across \
             the memories (empty {} if none are shared).",
        );
        prompt
    }

    fn parse_response(response: &str) -> CompactedMemory {
        // Try direct JSON parse.
        if let Ok(r) = serde_json::from_str::<CompactResponse>(response) {
            return CompactedMemory {
                content: r.content,
                metadata: r.metadata.unwrap_or_default(),
            };
        }

        // Strip markdown code fences and retry.
        let stripped = response
            .trim()
            .strip_prefix("```json")
            .or_else(|| response.trim().strip_prefix("```"))
            .and_then(|s| s.strip_suffix("```"))
            .map(|s| s.trim());

        if let Some(inner) = stripped {
            if let Ok(r) = serde_json::from_str::<CompactResponse>(inner) {
                return CompactedMemory {
                    content: r.content,
                    metadata: r.metadata.unwrap_or_default(),
                };
            }
        }

        // Fallback: treat entire response as content.
        CompactedMemory {
            content: response.to_string(),
            metadata: HashMap::new(),
        }
    }
}

#[async_trait]
impl Compactor for LlmCompactor {
    async fn synthesize(&self, memories: &[Memory]) -> Result<CompactedMemory, MastError> {
        let prompt = Self::build_prompt(memories);
        let response = (self.llm)(&prompt)
            .await
            .map_err(|e| MastError::Compaction(e))?;
        Ok(Self::parse_response(&response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Tier;
    use ulid::Ulid;

    fn make_memory(content: &str) -> Memory {
        Memory {
            id: Ulid::new(),
            collection: "test".into(),
            content: content.into(),
            embedding: vec![0.1],
            metadata: HashMap::new(),
            tier: Tier::Active,
            created_at: 1000,
            access_count: 0,
            last_accessed: 1000,
            ttl: None,
            compacted_at: None,
            parent_ids: vec![],
        }
    }

    #[tokio::test]
    async fn mock_compactor_concatenates() {
        let compactor = MockCompactor;
        let memories = vec![make_memory("one"), make_memory("two"), make_memory("three")];
        let result = compactor.synthesize(&memories).await.unwrap();
        assert_eq!(result.content, "one | two | three");
    }

    #[tokio::test]
    async fn mock_compactor_single() {
        let compactor = MockCompactor;
        let memories = vec![make_memory("only")];
        let result = compactor.synthesize(&memories).await.unwrap();
        assert_eq!(result.content, "only");
    }

    #[tokio::test]
    async fn llm_compactor_synthesizes() {
        let compactor = LlmCompactor::new(Box::new(|_prompt: &str| {
            Box::pin(async {
                Ok(r#"{"content": "combined summary", "metadata": {"source": "test"}}"#
                    .to_string())
            })
        }));
        let memories = vec![make_memory("first"), make_memory("second")];
        let result = compactor.synthesize(&memories).await.unwrap();
        assert_eq!(result.content, "combined summary");
        assert_eq!(result.metadata.get("source").unwrap(), "test");
    }

    #[tokio::test]
    async fn llm_compactor_fallback_plain_text() {
        let compactor = LlmCompactor::new(Box::new(|_prompt: &str| {
            Box::pin(async { Ok("just plain text response".to_string()) })
        }));
        let memories = vec![make_memory("a"), make_memory("b")];
        let result = compactor.synthesize(&memories).await.unwrap();
        assert_eq!(result.content, "just plain text response");
        assert!(result.metadata.is_empty());
    }

    #[tokio::test]
    async fn llm_compactor_error_propagates() {
        let compactor = LlmCompactor::new(Box::new(|_prompt: &str| {
            Box::pin(async { Err("LLM unavailable".to_string()) })
        }));
        let memories = vec![make_memory("x")];
        let result = compactor.synthesize(&memories).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, MastError::Compaction(msg) if msg == "LLM unavailable"),
        );
    }
}
