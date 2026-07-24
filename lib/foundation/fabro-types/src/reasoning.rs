use serde::{Deserialize, Serialize, de};

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReasoningOutput {
    /// Model-authored summary of its reasoning, safe to show to users.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
    /// Verbatim readable reasoning text, when the provider returns it in
    /// addition to (or instead of) a summary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    trace:   Option<String>,
}

impl ReasoningOutput {
    /// Creates reasoning output with both a model-authored summary and a
    /// verbatim trace.
    #[must_use]
    pub fn new(summary: impl Into<String>, trace: impl Into<String>) -> Self {
        Self {
            summary: Some(summary.into()),
            trace:   Some(trace.into()),
        }
    }

    /// Creates reasoning output containing only a model-authored summary.
    #[must_use]
    pub fn from_summary(summary: impl Into<String>) -> Self {
        Self {
            summary: Some(summary.into()),
            trace:   None,
        }
    }

    /// Creates reasoning output containing only a verbatim trace.
    #[must_use]
    pub fn from_trace(trace: impl Into<String>) -> Self {
        Self {
            summary: None,
            trace:   Some(trace.into()),
        }
    }

    /// Returns the model-authored summary, when present.
    #[must_use]
    pub fn summary(&self) -> Option<&str> {
        self.summary.as_deref()
    }

    /// Returns the verbatim readable reasoning trace, when present.
    #[must_use]
    pub fn trace(&self) -> Option<&str> {
        self.trace.as_deref()
    }
}

impl<'de> Deserialize<'de> for ReasoningOutput {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Fields {
            #[serde(default)]
            summary: Option<String>,
            #[serde(default)]
            trace:   Option<String>,
        }

        let Fields { summary, trace } = Fields::deserialize(deserializer)?;
        match (summary, trace) {
            (Some(summary), Some(trace)) => Ok(Self::new(summary, trace)),
            (Some(summary), None) => Ok(Self::from_summary(summary)),
            (None, Some(trace)) => Ok(Self::from_trace(trace)),
            (None, None) => Err(de::Error::custom(
                "reasoning output requires a summary or trace",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn summary_only_round_trips_without_trace_member() {
        let output = ReasoningOutput::from_summary("checked the parser first");
        let v = serde_json::to_value(&output).unwrap();
        assert_eq!(v, json!({"summary": "checked the parser first"}));
        assert_eq!(
            serde_json::from_value::<ReasoningOutput>(v).unwrap(),
            output
        );
    }

    #[test]
    fn trace_only_round_trips_without_summary_member() {
        let output = ReasoningOutput::from_trace("step one, step two");
        let v = serde_json::to_value(&output).unwrap();
        assert_eq!(v, json!({"trace": "step one, step two"}));
        assert_eq!(
            serde_json::from_value::<ReasoningOutput>(v).unwrap(),
            output
        );
    }

    #[test]
    fn both_fields_round_trip() {
        let output = ReasoningOutput::new("summary", "trace");
        let v = serde_json::to_value(&output).unwrap();
        assert_eq!(v, json!({"summary": "summary", "trace": "trace"}));
        assert_eq!(
            serde_json::from_value::<ReasoningOutput>(v).unwrap(),
            output
        );
    }

    #[test]
    fn empty_object_is_rejected() {
        let error = serde_json::from_value::<ReasoningOutput>(json!({})).unwrap_err();
        assert!(error.to_string().contains("requires a summary or trace"));
    }

    #[test]
    fn explicit_nulls_are_rejected() {
        let error =
            serde_json::from_value::<ReasoningOutput>(json!({"summary": null, "trace": null}))
                .unwrap_err();
        assert!(error.to_string().contains("requires a summary or trace"));
    }
}
