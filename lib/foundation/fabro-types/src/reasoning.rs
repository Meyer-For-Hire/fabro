use serde::{Deserialize, Serialize};

/// Readable model reasoning normalized into a provider-neutral shape.
///
/// Providers expose reasoning through several unrelated channels: OpenAI
/// Responses reasoning items, OpenAI-compatible `reasoning_details`, and
/// flattened `reasoning`/`reasoning_content`/`thinking` strings. This type
/// reduces all of them to the two capabilities consumers actually care
/// about, so the durable event contract does not change shape when a
/// provider dialect does.
///
/// Both fields may be populated for the same response. An emitted object
/// always carries at least one of them; opaque provider material
/// (signatures, IDs, encrypted or redacted payloads) never appears here.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReasoningOutput {
    /// Model-authored summary of its reasoning, safe to show to users.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Verbatim readable reasoning text, when the provider returns it in
    /// addition to (or instead of) a summary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace:   Option<String>,
}

impl ReasoningOutput {
    /// Returns `true` when neither readable field is present, meaning the
    /// object carries nothing worth emitting.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.summary.is_none() && self.trace.is_none()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn summary_only_round_trips_without_trace_member() {
        let output = ReasoningOutput {
            summary: Some("checked the parser first".to_string()),
            trace:   None,
        };
        let v = serde_json::to_value(&output).unwrap();
        assert_eq!(v, json!({"summary": "checked the parser first"}));
        assert_eq!(
            serde_json::from_value::<ReasoningOutput>(v).unwrap(),
            output
        );
    }

    #[test]
    fn trace_only_round_trips_without_summary_member() {
        let output = ReasoningOutput {
            summary: None,
            trace:   Some("step one, step two".to_string()),
        };
        let v = serde_json::to_value(&output).unwrap();
        assert_eq!(v, json!({"trace": "step one, step two"}));
        assert_eq!(
            serde_json::from_value::<ReasoningOutput>(v).unwrap(),
            output
        );
    }

    #[test]
    fn both_fields_round_trip() {
        let output = ReasoningOutput {
            summary: Some("summary".to_string()),
            trace:   Some("trace".to_string()),
        };
        let v = serde_json::to_value(&output).unwrap();
        assert_eq!(v, json!({"summary": "summary", "trace": "trace"}));
        assert_eq!(
            serde_json::from_value::<ReasoningOutput>(v).unwrap(),
            output
        );
    }

    #[test]
    fn absent_members_are_omitted_rather_than_null() {
        let v = serde_json::to_value(ReasoningOutput::default()).unwrap();
        assert_eq!(v, json!({}));
        assert!(ReasoningOutput::default().is_empty());
    }

    #[test]
    fn explicit_nulls_deserialize_as_absent() {
        let output: ReasoningOutput =
            serde_json::from_value(json!({"summary": null, "trace": null})).unwrap();
        assert!(output.is_empty());
    }
}
