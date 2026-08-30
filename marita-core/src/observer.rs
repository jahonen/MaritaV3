//! Observer-pipeline selection and runtime configuration.

use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObserverModel {
    Legacy,
    Causal,
}

impl FromStr for ObserverModel {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "legacy" => Ok(Self::Legacy),
            "causal" => Ok(Self::Causal),
            _ => Err(format!(
                "unknown observer model '{value}'; expected legacy or causal"
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ObserverConfig {
    pub model: ObserverModel,
    pub history_au: f64,
    pub history_budget_bytes: usize,
    pub max_passive_candidates: usize,
}

impl Default for ObserverConfig {
    fn default() -> Self {
        Self {
            model: ObserverModel::Legacy,
            history_au: 100.0,
            history_budget_bytes: 512 * 1024 * 1024,
            max_passive_candidates: 4096,
        }
    }
}
