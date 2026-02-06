use super::*;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub(super) struct RuntimeConfig {
    pub(super) ask: AskRuntimeConfig,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            ask: AskRuntimeConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub(super) struct AskRuntimeConfig {
    pub(super) neighbor_limit: usize,
    pub(super) snippet_count_in_answer: usize,
    pub(super) edge_weight: AskEdgeWeightConfig,
}

impl Default for AskRuntimeConfig {
    fn default() -> Self {
        Self {
            neighbor_limit: 5,
            snippet_count_in_answer: 2,
            edge_weight: AskEdgeWeightConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub(super) struct AskEdgeWeightConfig {
    pub(super) depends_on: f64,
    pub(super) tests: f64,
    pub(super) refines: f64,
    pub(super) impacts: f64,
    pub(super) conflicts_with: f64,
}

impl Default for AskEdgeWeightConfig {
    fn default() -> Self {
        Self {
            depends_on: 1.0,
            tests: 0.8,
            refines: 0.7,
            impacts: 0.6,
            conflicts_with: 1.2,
        }
    }
}

pub(super) fn load_runtime_config() -> RuntimeConfig {
    let path = Path::new(".foundry/config.json");
    let raw = match fs::read_to_string(path) {
        Ok(v) => v,
        Err(_) => return RuntimeConfig::default(),
    };
    serde_json::from_str::<RuntimeConfig>(&raw).unwrap_or_default()
}
