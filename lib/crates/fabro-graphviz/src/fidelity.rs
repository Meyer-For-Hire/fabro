use serde::{Deserialize, Serialize};
use strum::{Display, EnumString, VariantArray};

/// Fidelity mode controlling how much prior context is provided to LLM
/// sessions.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    Hash,
    Display,
    EnumString,
    VariantArray,
    Serialize,
    Deserialize,
)]
#[strum(serialize_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum Fidelity {
    /// Complete context, no summarization — sessions share a thread.
    Full,
    /// Minimal: only graph goal and run ID.
    Truncate,
    /// Structured nested-bullet summary (default).
    #[default]
    Compact,
    /// Brief textual summary (~600 token target).
    #[strum(serialize = "summary:low")]
    #[serde(rename = "summary:low")]
    SummaryLow,
    /// Moderate textual summary (~1500 token target).
    #[strum(serialize = "summary:medium")]
    #[serde(rename = "summary:medium")]
    SummaryMedium,
    /// Detailed per-stage Markdown report.
    #[strum(serialize = "summary:high")]
    #[serde(rename = "summary:high")]
    SummaryHigh,
}

impl Fidelity {
    /// All supported fidelity modes in display order.
    #[must_use]
    pub fn variants() -> &'static [Self] {
        Self::VARIANTS
    }

    /// Degrade full fidelity to summary:high (used on checkpoint resume).
    #[must_use]
    pub fn degraded(self) -> Self {
        match self {
            Self::Full => Self::SummaryHigh,
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fidelity_display_roundtrips() {
        for mode in Fidelity::variants() {
            let s = mode.to_string();
            let parsed: Fidelity = s.parse().unwrap();
            assert_eq!(parsed, *mode);
        }
    }

    #[test]
    fn fidelity_default_is_compact() {
        assert_eq!(Fidelity::default(), Fidelity::Compact);
    }

    #[test]
    fn fidelity_degraded_full_becomes_summary_high() {
        assert_eq!(Fidelity::Full.degraded(), Fidelity::SummaryHigh);
    }

    #[test]
    fn fidelity_degraded_non_full_unchanged() {
        assert_eq!(Fidelity::Compact.degraded(), Fidelity::Compact);
        assert_eq!(Fidelity::SummaryHigh.degraded(), Fidelity::SummaryHigh);
    }

    #[test]
    fn fidelity_unknown_mode_errors() {
        assert!("bogus".parse::<Fidelity>().is_err());
    }

    #[test]
    fn fidelity_serde_matches_strum_display() {
        for mode in Fidelity::variants() {
            let json = serde_json::to_value(mode).unwrap();
            assert_eq!(json, serde_json::Value::String(mode.to_string()));
            let parsed: Fidelity = serde_json::from_value(json).unwrap();
            assert_eq!(parsed, *mode);
        }
    }
}
