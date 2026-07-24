//! Run-scoped stage execution identity.
//!
//! A *stage execution* is one top-level handler invocation of a node that
//! became observable within a run. Its 1-based ordinal is the numeric
//! component of the external `StageId` (`node_id@N`). The ordinal is distinct
//! from the *graph visit* (how many times workflow control entered the node,
//! which drives `max_visits` and checkpoints) and from the *handler attempt*
//! (automatic retries inside one execution).
//!
//! The tracker is deliberately not checkpointed: its durable source of truth
//! is the append-only stage event history. On resume it is seeded from the
//! run projection's per-node maxima, so a reexecuted in-flight node allocates
//! the next unused ordinal instead of mutating the cancelled execution.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use fabro_types::{RunProjection, StageId};

/// One reserved stage execution: the identity of a single resumable handler
/// invocation of a node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StageExecution {
    /// 1-based execution ordinal; becomes the `@N` in the external `StageId`.
    pub ordinal:      u32,
    /// Graph visit that produced this execution.
    pub graph_visit:  u32,
    /// Prior execution this one resumes from, when the node had an observable
    /// post-checkpoint execution before the run was interrupted.
    pub resumed_from: Option<StageId>,
}

/// Seed data for the [`StageExecutionTracker`], derived from the run
/// projection when a run is resumed. A fresh run uses the default (empty)
/// seed; new run IDs own a new ordinal sequence.
#[derive(Clone, Debug, Default)]
pub struct StageExecutionSeed {
    /// Highest execution ordinal already observable per node.
    pub high_water:   HashMap<String, u32>,
    /// Latest post-checkpoint execution per node; the next reservation for
    /// that node links back to it via `resumed_from_stage_id`.
    pub resumed_from: HashMap<String, StageId>,
}

impl StageExecutionSeed {
    /// Build the seed from the run projection at resume time.
    ///
    /// `checkpoint_seq` is the event sequence number of the selected
    /// checkpoint. Only stages that first became observable *after* that
    /// checkpoint are eligible provenance targets: an older execution with the
    /// same node ID completed before the checkpoint and is not what the
    /// resumed invocation continues from.
    #[must_use]
    pub fn from_projection(projection: &RunProjection, checkpoint_seq: u32) -> Self {
        let mut high_water: HashMap<String, u32> = HashMap::new();
        let mut resumed_from: HashMap<String, StageId> = HashMap::new();
        // `iter_stages` yields chronological `first_event_seq` order, so a
        // later insert per node retains the latest post-checkpoint execution.
        for (stage_id, stage) in projection.iter_stages() {
            let node_id = stage_id.node_id();
            let entry = high_water.entry(node_id.to_string()).or_default();
            *entry = (*entry).max(stage_id.visit());
            if stage.first_event_seq.get() > checkpoint_seq {
                resumed_from.insert(node_id.to_string(), stage_id.clone());
            }
        }
        Self {
            high_water,
            resumed_from,
        }
    }
}

#[derive(Debug, Default)]
struct TrackerState {
    /// Highest ordinal observed or reserved per node.
    high_water:   HashMap<String, u32>,
    /// Pending provenance links, consumed by the first reservation per node.
    resumed_from: HashMap<String, StageId>,
    /// Active execution scope per node. Cleared at the node boundary and
    /// replaced by the next reservation.
    active:       HashMap<String, StageExecution>,
}

/// Cloneable, run-scoped allocator for stage execution ordinals. Clones share
/// one synchronized state so the core lifecycle and direct-dispatch handlers
/// (parallel branches) allocate from the same sequence.
#[derive(Clone, Debug, Default)]
pub(crate) struct StageExecutionTracker {
    state: Arc<Mutex<TrackerState>>,
}

impl StageExecutionTracker {
    #[must_use]
    pub(crate) fn seeded(seed: StageExecutionSeed) -> Self {
        Self {
            state: Arc::new(Mutex::new(TrackerState {
                high_water:   seed.high_water,
                resumed_from: seed.resumed_from,
                active:       HashMap::new(),
            })),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, TrackerState> {
        self.state
            .lock()
            .expect("stage execution tracker mutex is never poisoned: no code panics while holding this lock")
    }

    /// Clear the node's prior execution scope at the node boundary. The next
    /// `reserve`/`ensure` call allocates a fresh ordinal; a reservation is not
    /// made here so that a StageStart hook block or process exit before any
    /// stage-scoped event leaves no phantom execution.
    pub(crate) fn begin_node(&self, node_id: &str) {
        self.lock().active.remove(node_id);
    }

    /// The node's active execution scope, if one has been reserved since the
    /// last node boundary.
    pub(crate) fn active(&self, node_id: &str) -> Option<StageExecution> {
        self.lock().active.get(node_id).cloned()
    }

    /// Allocate the next execution ordinal for the node and make it the active
    /// scope. Consumes the node's pending provenance link, if any.
    pub(crate) fn reserve(&self, node_id: &str, graph_visit: u32) -> StageExecution {
        let mut state = self.lock();
        let entry = state.high_water.entry(node_id.to_string()).or_default();
        *entry = entry.saturating_add(1);
        let ordinal = *entry;
        let resumed_from = state.resumed_from.remove(node_id);
        let execution = StageExecution {
            ordinal,
            graph_visit,
            resumed_from,
        };
        state.active.insert(node_id.to_string(), execution.clone());
        execution
    }

    /// The active scope for the node, reserving one only when none exists.
    /// Later attempts within one execution and checkpoint pre-steps reuse the
    /// first attempt's reservation.
    pub(crate) fn ensure(&self, node_id: &str, graph_visit: u32) -> StageExecution {
        if let Some(execution) = self.active(node_id) {
            return execution;
        }
        self.reserve(node_id, graph_visit)
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use chrono::Utc;
    use fabro_types::{Graph, RunId, RunSpec, StageId, WorkflowSettings, test_support};

    use super::*;

    fn projection_with_stages(stages: &[(&str, u32, u32)]) -> RunProjection {
        let spec = RunSpec {
            run_id:           RunId::new(),
            settings:         WorkflowSettings::default(),
            graph:            Graph::new("test"),
            graph_source:     None,
            workflow_slug:    None,
            automation:       None,
            source_directory: None,
            labels:           std::collections::HashMap::new(),
            provenance:       test_support::test_run_provenance(),
            manifest_blob:    None,
            definition_blob:  None,
            git:              None,
            fork_source_ref:  None,
        };
        let mut projection = RunProjection::new(String::new(), spec, Utc::now());
        for (node_id, visit, seq) in stages {
            projection.stage_entry(
                node_id,
                *visit,
                NonZeroU32::new(*seq).expect("test seq must be non-zero"),
            );
        }
        projection
    }

    #[test]
    fn reserve_starts_at_one_and_allocates_monotonically_per_node() {
        let tracker = StageExecutionTracker::default();

        assert_eq!(tracker.reserve("work", 1).ordinal, 1);
        tracker.begin_node("work");
        assert_eq!(tracker.reserve("work", 2).ordinal, 2);
        assert_eq!(tracker.reserve("other", 1).ordinal, 1);
    }

    #[test]
    fn seeds_from_projection_maxima() {
        let projection = projection_with_stages(&[("work", 1, 2), ("work", 2, 5), ("plan", 1, 3)]);
        let seed = StageExecutionSeed::from_projection(&projection, 0);
        let tracker = StageExecutionTracker::seeded(seed);

        assert_eq!(tracker.reserve("work", 1).ordinal, 3);
        assert_eq!(tracker.reserve("plan", 1).ordinal, 2);
        assert_eq!(tracker.reserve("new", 1).ordinal, 1);
    }

    #[test]
    fn graph_visit_and_ordinal_can_diverge() {
        let projection = projection_with_stages(&[("work", 1, 2), ("work", 2, 5)]);
        let seed = StageExecutionSeed::from_projection(&projection, 0);
        let tracker = StageExecutionTracker::seeded(seed);

        let execution = tracker.reserve("work", 2);
        assert_eq!(execution.ordinal, 3);
        assert_eq!(execution.graph_visit, 2);
    }

    #[test]
    fn ensure_reuses_active_reservation_across_attempts() {
        let tracker = StageExecutionTracker::default();

        let first = tracker.ensure("work", 1);
        let second = tracker.ensure("work", 1);
        assert_eq!(first, second);
        assert_eq!(second.ordinal, 1);

        tracker.begin_node("work");
        assert_eq!(tracker.ensure("work", 2).ordinal, 2);
    }

    #[test]
    fn begin_node_clears_only_that_node() {
        let tracker = StageExecutionTracker::default();
        tracker.reserve("work", 1);
        tracker.reserve("verify", 1);

        tracker.begin_node("work");

        assert_eq!(tracker.active("work"), None);
        assert_eq!(tracker.active("verify").map(|e| e.ordinal), Some(1));
    }

    #[test]
    fn provenance_only_selects_stages_after_the_checkpoint() {
        let projection = projection_with_stages(&[("work", 1, 2), ("work", 2, 8), ("plan", 1, 3)]);
        let seed = StageExecutionSeed::from_projection(&projection, 5);

        assert_eq!(
            seed.resumed_from.get("work"),
            Some(&StageId::new("work", 2))
        );
        assert_eq!(seed.resumed_from.get("plan"), None);
    }

    #[test]
    fn first_reservation_consumes_provenance() {
        let projection = projection_with_stages(&[("work", 1, 6)]);
        let seed = StageExecutionSeed::from_projection(&projection, 5);
        let tracker = StageExecutionTracker::seeded(seed);

        let first = tracker.reserve("work", 1);
        assert_eq!(first.ordinal, 2);
        assert_eq!(first.resumed_from, Some(StageId::new("work", 1)));

        tracker.begin_node("work");
        let second = tracker.reserve("work", 2);
        assert_eq!(second.ordinal, 3);
        assert_eq!(second.resumed_from, None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn concurrent_reservations_stay_unique_per_node() {
        let tracker = StageExecutionTracker::default();
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let tracker = tracker.clone();
                tokio::spawn(async move { tracker.reserve("branch", 1).ordinal })
            })
            .collect();

        let mut ordinals = Vec::new();
        for handle in handles {
            ordinals.push(handle.await.expect("reservation task panicked"));
        }
        ordinals.sort_unstable();
        assert_eq!(ordinals, (1..=8).collect::<Vec<_>>());
    }
}
