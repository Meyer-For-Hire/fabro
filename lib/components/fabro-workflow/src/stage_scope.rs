use fabro_types::{ParallelBranchId, StageId};

use crate::context::{Context as WfContext, WorkflowContext, keys};
use crate::run_dir::visit_from_context;

/// Read the stage execution ordinal seeded by the workflow lifecycle (or a
/// parallel branch dispatch). `None` when the current node has not reserved an
/// execution yet — direct-handler call sites (tests, etc.) that skip the full
/// lifecycle fall back to the graph visit, which equals the ordinal for a
/// first execution.
fn execution_ordinal_from_context(context: &WfContext) -> Option<u32> {
    context
        .get(keys::INTERNAL_STAGE_EXECUTION_ORDINAL)
        .and_then(|value| value.as_u64())
        .map(|ordinal| u32::try_from(ordinal).unwrap_or(u32::MAX))
}

/// Stage-level scope threaded through event emission to populate
/// `stage_id` / `parallel_group_id` / `parallel_branch_id` on events
/// that happen inside a concrete stage execution.
///
/// `visit` is the 1-based stage execution ordinal — the numeric component of
/// the external `StageId`. It matches the graph visit for a first execution
/// and diverges when a cancelled or crashed invocation is reexecuted after
/// resume.
#[derive(Clone, Debug)]
pub struct StageScope {
    pub node_id:            String,
    pub visit:              u32,
    pub parallel_group_id:  Option<StageId>,
    pub parallel_branch_id: Option<ParallelBranchId>,
}

impl StageScope {
    /// Build a scope from the given node id, sourcing the execution ordinal
    /// and parallel ids from the current context.
    pub fn from_context(context: &WfContext, node_id: impl Into<String>) -> Self {
        let visit = execution_ordinal_from_context(context)
            .unwrap_or_else(|| u32::try_from(visit_from_context(context)).unwrap_or(u32::MAX));
        Self {
            node_id: node_id.into(),
            visit,
            parallel_group_id: context.parallel_group_id(),
            parallel_branch_id: context.parallel_branch_id(),
        }
    }

    /// Build scope for a handler invocation. Prefers the `current_stage_scope`
    /// seeded by the fidelity lifecycle `before_node` hook, and falls back to
    /// synthesizing one from `node_id` for direct-handler call sites (tests,
    /// etc.) that don't go through the full lifecycle.
    pub fn for_handler(context: &WfContext, node_id: impl Into<String>) -> Self {
        context
            .current_stage_scope()
            .unwrap_or_else(|| Self::from_context(context, node_id))
    }

    /// Build scope for the branch-lifecycle events emitted by the parallel
    /// handler (`ParallelBranchStarted`, `ParallelBranchCompleted`, and the
    /// pre-dispatch `GitCommit` for the branch worktree).
    ///
    /// `target_visit` is the branch target's stage execution ordinal for this
    /// particular dispatch, reserved through the run's shared
    /// `StageExecutionTracker` so a resumed fan-out gets a fresh child
    /// identity instead of overwriting the cancelled attempt's.
    #[must_use]
    pub fn for_parallel_branch(
        target_node_id: impl Into<String>,
        target_visit: u32,
        parallel_group_id: StageId,
        parallel_branch_id: ParallelBranchId,
    ) -> Self {
        Self {
            node_id:            target_node_id.into(),
            visit:              target_visit,
            parallel_group_id:  Some(parallel_group_id),
            parallel_branch_id: Some(parallel_branch_id),
        }
    }

    #[must_use]
    pub fn stage_id(&self) -> StageId {
        StageId::new(self.node_id.clone(), self.visit)
    }
}
