use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::StageOutcome;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParallelBranchResult {
    pub id:              String,
    pub status:          StageOutcome,
    #[serde(default)]
    pub context_updates: BTreeMap<String, Value>,
}
