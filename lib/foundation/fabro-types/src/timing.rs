//! Wall and active timing primitives shared across stages and runs.
//!
//! Two value objects: [`StageTiming`] for one stage visit, [`RunTiming`] for a
//! run-level rollup. Both expose the same four fields:
//!
//! - `wall_time_ms`: elapsed clock time from start to finish.
//! - `inference_time_ms`: Fabro-observed LLM request/stream elapsed time.
//! - `tool_time_ms`: tool or command execution elapsed time.
//! - `active_time_ms`: `inference_time_ms + tool_time_ms`.
//!
//! `active_time_ms` is precomputed and serialized so API consumers do not need
//! to redo the addition. Use the `new` constructors to enforce the invariant.
//!
//! For parallel container stages the container reports `active = 0` and the
//! child branches carry their own work timing; run-level active time sums work
//! across stage visits and can exceed run wall time when work runs in parallel.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Non-negative milliseconds between two instants.
///
/// Clock skew or an out-of-order replay can put `end` before `start`; those
/// spans contribute zero rather than wrapping into a large unsigned value.
#[must_use]
pub fn elapsed_ms(start: DateTime<Utc>, end: DateTime<Utc>) -> u64 {
    u64::try_from(end.signed_duration_since(start).num_milliseconds().max(0))
        .expect("non-negative chrono millisecond durations fit in u64")
}

/// Timing breakdown for one stage visit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageTiming {
    /// Wall-clock time from stage start to terminal event.
    pub wall_time_ms:      u64,
    /// Fabro-observed LLM request/stream elapsed time.
    #[serde(default)]
    pub inference_time_ms: u64,
    /// Tool or command execution elapsed time.
    #[serde(default)]
    pub tool_time_ms:      u64,
    /// `inference_time_ms + tool_time_ms`.
    pub active_time_ms:    u64,
}

impl StageTiming {
    /// Construct a [`StageTiming`] with `active_time_ms` derived from the
    /// breakdown.
    #[must_use]
    pub fn new(wall_time_ms: u64, inference_time_ms: u64, tool_time_ms: u64) -> Self {
        let active_time_ms = inference_time_ms.saturating_add(tool_time_ms);
        Self {
            wall_time_ms,
            inference_time_ms,
            tool_time_ms,
            active_time_ms,
        }
    }

    /// Stages with no inference/tool work (human, wait, conditional, start,
    /// exit, parallel container) report wall time only.
    #[must_use]
    pub fn wall_only(wall_time_ms: u64) -> Self {
        Self::new(wall_time_ms, 0, 0)
    }

    /// Active-only timing for stages whose wall time will be supplied
    /// separately by the executor's own stopwatch (current shape of the
    /// handler → executor hop).
    #[must_use]
    pub fn active_only(inference_time_ms: u64, tool_time_ms: u64) -> Self {
        Self::new(0, inference_time_ms, tool_time_ms)
    }

    /// Scale the breakdown down so `active_time_ms` does not exceed
    /// `wall_time_ms`, preserving the inference/tool ratio.
    ///
    /// Only meaningful for live estimates of a single in-flight stage, where
    /// an open bracket left behind by a killed worker would otherwise tick up
    /// without bound. Finalized timings come from the worker's stopwatch and
    /// already satisfy the invariant.
    ///
    /// Deliberately *not* applied at run level: concurrent branches can
    /// legitimately sum past run wall time.
    #[must_use]
    pub fn clamped_to_wall(&self) -> Self {
        let active_time_ms = u128::from(self.inference_time_ms) + u128::from(self.tool_time_ms);
        if active_time_ms <= u128::from(self.wall_time_ms) {
            return *self;
        }
        // Preserve the split rather than truncating one side, so a clamped
        // stage still shows where its time went. Widen for the multiply: the
        // quotient is bounded by `wall_time_ms` because the exact, widened
        // active total exceeds it here, so it always fits back into u64.
        let scaled =
            u128::from(self.inference_time_ms) * u128::from(self.wall_time_ms) / active_time_ms;
        let inference_time_ms =
            u64::try_from(scaled).expect("scaled inference time is bounded by wall time");
        let tool_time_ms = self.wall_time_ms.saturating_sub(inference_time_ms);
        Self::new(self.wall_time_ms, inference_time_ms, tool_time_ms)
    }

    /// Sum two timings field-by-field. Used to aggregate visits of one node
    /// and to accumulate run-level rollups.
    #[must_use]
    pub fn saturating_add(&self, other: &Self) -> Self {
        Self::new(
            self.wall_time_ms.saturating_add(other.wall_time_ms),
            self.inference_time_ms
                .saturating_add(other.inference_time_ms),
            self.tool_time_ms.saturating_add(other.tool_time_ms),
        )
    }
}

/// Timing rollup for an entire run.
///
/// `wall_time_ms` is the run's clock duration from start to terminal event.
/// The other three fields sum work across stage visits, so `active_time_ms`
/// can exceed `wall_time_ms` when parallel branches run concurrently.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunTiming {
    /// Wall-clock time from run start to terminal event.
    pub wall_time_ms:      u64,
    /// Sum of inference time across every stage visit.
    #[serde(default)]
    pub inference_time_ms: u64,
    /// Sum of tool time across every stage visit.
    #[serde(default)]
    pub tool_time_ms:      u64,
    /// `inference_time_ms + tool_time_ms`.
    pub active_time_ms:    u64,
}

impl RunTiming {
    /// Construct a [`RunTiming`] with `active_time_ms` derived from the
    /// breakdown.
    #[must_use]
    pub fn new(wall_time_ms: u64, inference_time_ms: u64, tool_time_ms: u64) -> Self {
        let active_time_ms = inference_time_ms.saturating_add(tool_time_ms);
        Self {
            wall_time_ms,
            inference_time_ms,
            tool_time_ms,
            active_time_ms,
        }
    }

    /// Run-level rollups for runs with no inference/tool work yet recorded.
    #[must_use]
    pub fn wall_only(wall_time_ms: u64) -> Self {
        Self::new(wall_time_ms, 0, 0)
    }

    /// Sum two run timings field-by-field. Used to accumulate aggregate
    /// billing totals across completed runs.
    #[must_use]
    pub fn saturating_add(&self, other: &Self) -> Self {
        Self::new(
            self.wall_time_ms.saturating_add(other.wall_time_ms),
            self.inference_time_ms
                .saturating_add(other.inference_time_ms),
            self.tool_time_ms.saturating_add(other.tool_time_ms),
        )
    }

    /// Return a copy with `wall_time_ms` replaced. Useful at finalize time
    /// when the active breakdown is summed across stage visits but the run
    /// wall time is the executor's clock duration.
    #[must_use]
    pub fn with_wall_time(self, wall_time_ms: u64) -> Self {
        Self {
            wall_time_ms,
            ..self
        }
    }
}

impl From<StageTiming> for RunTiming {
    fn from(stage: StageTiming) -> Self {
        Self {
            wall_time_ms:      stage.wall_time_ms,
            inference_time_ms: stage.inference_time_ms,
            tool_time_ms:      stage.tool_time_ms,
            active_time_ms:    stage.active_time_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::{RunTiming, StageTiming, elapsed_ms};

    #[test]
    fn stage_timing_new_derives_active_as_sum_of_inference_and_tool() {
        let timing = StageTiming::new(1000, 200, 300);
        assert_eq!(timing.wall_time_ms, 1000);
        assert_eq!(timing.inference_time_ms, 200);
        assert_eq!(timing.tool_time_ms, 300);
        assert_eq!(timing.active_time_ms, 500);
    }

    #[test]
    fn stage_timing_wall_only_zeroes_breakdown_and_active() {
        let timing = StageTiming::wall_only(750);
        assert_eq!(timing.wall_time_ms, 750);
        assert_eq!(timing.inference_time_ms, 0);
        assert_eq!(timing.tool_time_ms, 0);
        assert_eq!(timing.active_time_ms, 0);
    }

    #[test]
    fn stage_timing_saturating_add_sums_all_breakdown_fields() {
        let a = StageTiming::new(100, 30, 70);
        let b = StageTiming::new(200, 50, 25);
        let sum = a.saturating_add(&b);
        assert_eq!(sum.wall_time_ms, 300);
        assert_eq!(sum.inference_time_ms, 80);
        assert_eq!(sum.tool_time_ms, 95);
        assert_eq!(sum.active_time_ms, 175);
    }

    #[test]
    fn stage_timing_clamp_uses_the_unsaturated_active_total() {
        let timing = StageTiming::new(u64::MAX, u64::MAX, u64::MAX).clamped_to_wall();

        assert_eq!(timing.inference_time_ms, u64::MAX / 2);
        assert_eq!(timing.tool_time_ms, u64::MAX.saturating_sub(u64::MAX / 2));
        assert_eq!(timing.active_time_ms, u64::MAX);
    }

    #[test]
    fn elapsed_ms_clamps_out_of_order_instants_to_zero() {
        let later = Utc.timestamp_opt(100, 0).unwrap();
        let earlier = Utc.timestamp_opt(99, 0).unwrap();

        assert_eq!(elapsed_ms(later, earlier), 0);
    }

    #[test]
    fn run_timing_wall_only_zeroes_breakdown_and_active() {
        let timing = RunTiming::wall_only(1500);
        assert_eq!(timing.wall_time_ms, 1500);
        assert_eq!(timing.inference_time_ms, 0);
        assert_eq!(timing.tool_time_ms, 0);
        assert_eq!(timing.active_time_ms, 0);
    }

    #[test]
    fn run_timing_saturating_add_sums_all_breakdown_fields() {
        let a = RunTiming::new(1000, 100, 50);
        let b = RunTiming::new(500, 200, 150);
        let sum = a.saturating_add(&b);
        assert_eq!(sum.wall_time_ms, 1500);
        assert_eq!(sum.inference_time_ms, 300);
        assert_eq!(sum.tool_time_ms, 200);
        assert_eq!(sum.active_time_ms, 500);
    }

    #[test]
    fn run_timing_from_stage_timing_copies_all_fields() {
        let stage = StageTiming::new(900, 200, 300);
        let run: RunTiming = stage.into();
        assert_eq!(run.wall_time_ms, 900);
        assert_eq!(run.inference_time_ms, 200);
        assert_eq!(run.tool_time_ms, 300);
        assert_eq!(run.active_time_ms, 500);
    }

    #[test]
    fn run_timing_with_wall_time_replaces_wall_and_preserves_breakdown() {
        let original = RunTiming::new(1000, 200, 300);
        let updated = original.with_wall_time(5000);
        assert_eq!(updated.wall_time_ms, 5000);
        assert_eq!(updated.inference_time_ms, 200);
        assert_eq!(updated.tool_time_ms, 300);
        assert_eq!(updated.active_time_ms, 500);
    }

    #[test]
    fn stage_timing_round_trips_json_with_serialized_active_time() {
        let original = StageTiming::new(900, 250, 350);
        let json = serde_json::to_value(original).unwrap();
        assert_eq!(json["wall_time_ms"], 900);
        assert_eq!(json["inference_time_ms"], 250);
        assert_eq!(json["tool_time_ms"], 350);
        assert_eq!(json["active_time_ms"], 600);
        let parsed: StageTiming = serde_json::from_value(json).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn run_timing_round_trips_json_with_serialized_active_time() {
        let original = RunTiming::new(2000, 600, 400);
        let json = serde_json::to_value(original).unwrap();
        assert_eq!(json["wall_time_ms"], 2000);
        assert_eq!(json["inference_time_ms"], 600);
        assert_eq!(json["tool_time_ms"], 400);
        assert_eq!(json["active_time_ms"], 1000);
        let parsed: RunTiming = serde_json::from_value(json).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn stage_timing_breakdown_fields_default_when_missing_from_json() {
        let json = serde_json::json!({
            "wall_time_ms": 500,
            "active_time_ms": 0
        });
        let parsed: StageTiming = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.wall_time_ms, 500);
        assert_eq!(parsed.inference_time_ms, 0);
        assert_eq!(parsed.tool_time_ms, 0);
        assert_eq!(parsed.active_time_ms, 0);
    }
}
