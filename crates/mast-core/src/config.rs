use crate::types::{TierWeights, TimeDecay};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Durability {
    /// fsync on every commit (safe, slower)
    #[default]
    Immediate,
    /// OS decides when to flush (faster, data may be lost on crash)
    Eventual,
}

#[derive(Debug, Clone)]
pub struct MastConfig {
    pub db_path: PathBuf,
    pub default_collection: String,
    pub default_limit: usize,
    pub default_dimensions: usize,
    pub default_tier_weights: TierWeights,
    pub default_time_decay: Option<TimeDecay>,
    pub durability: Durability,
    /// Score multiplier for memories that were compacted into a newer memory.
    /// `0.0` or less removes them from recall.
    pub compacted_source_weight: f32,
}

impl Default for MastConfig {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("mast.db"),
            default_collection: "default".to_string(),
            default_limit: 10,
            default_dimensions: 1536,
            default_tier_weights: TierWeights::default(),
            default_time_decay: None,
            durability: Durability::default(),
            compacted_source_weight: 0.5,
        }
    }
}

impl MastConfig {
    pub fn with_db_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.db_path = path.into();
        self
    }

    pub fn with_default_collection(mut self, collection: impl Into<String>) -> Self {
        self.default_collection = collection.into();
        self
    }

    pub fn with_default_limit(mut self, limit: usize) -> Self {
        self.default_limit = limit;
        self
    }

    pub fn with_default_dimensions(mut self, dimensions: usize) -> Self {
        self.default_dimensions = dimensions;
        self
    }

    pub fn with_default_tier_weights(mut self, weights: TierWeights) -> Self {
        self.default_tier_weights = weights;
        self
    }

    pub fn with_default_time_decay(mut self, decay: TimeDecay) -> Self {
        self.default_time_decay = Some(decay);
        self
    }

    pub fn with_durability(mut self, durability: Durability) -> Self {
        self.durability = durability;
        self
    }

    pub fn with_compacted_source_weight(mut self, weight: f32) -> Self {
        self.compacted_source_weight = weight;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = MastConfig::default();
        assert_eq!(config.db_path, PathBuf::from("mast.db"));
        assert_eq!(config.default_collection, "default");
        assert_eq!(config.default_limit, 10);
        assert_eq!(config.default_dimensions, 1536);
    }

    #[test]
    fn config_builder_methods() {
        let config = MastConfig::default()
            .with_db_path("/tmp/test.db")
            .with_default_collection("my_col")
            .with_default_limit(20)
            .with_default_dimensions(768);
        assert_eq!(config.db_path, PathBuf::from("/tmp/test.db"));
        assert_eq!(config.default_collection, "my_col");
        assert_eq!(config.default_limit, 20);
        assert_eq!(config.default_dimensions, 768);
    }
}
