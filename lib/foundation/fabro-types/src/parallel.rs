use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::StageOutcome;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParallelBranchResult {
    pub id:              String,
    /// Zero-based input or outgoing-edge position. Absent only on records
    /// written before branch indexes became part of result identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index:           Option<usize>,
    /// Human-readable runtime item identity for `for_each` results.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_label:      Option<String>,
    pub status:          StageOutcome,
    #[serde(default)]
    pub context_updates: BTreeMap<String, Value>,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::ParallelBranchResult;
    use crate::StageOutcome;

    #[test]
    fn indexed_result_round_trips_with_item_label() {
        let result = ParallelBranchResult {
            id:              "review".to_string(),
            index:           Some(3),
            item_label:      Some("api".to_string()),
            status:          StageOutcome::Succeeded,
            context_updates: BTreeMap::default(),
        };

        let value = serde_json::to_value(&result).unwrap();
        assert_eq!(value["index"], 3);
        assert_eq!(value["item_label"], "api");
        assert_eq!(
            serde_json::from_value::<ParallelBranchResult>(value).unwrap(),
            result
        );
    }

    #[test]
    fn legacy_result_without_index_or_label_still_deserializes() {
        let result: ParallelBranchResult = serde_json::from_value(json!({
            "id": "review",
            "status": "succeeded",
            "context_updates": {}
        }))
        .unwrap();

        assert_eq!(result.index, None);
        assert_eq!(result.item_label, None);
    }
}
