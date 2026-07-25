import { LlmOutputKind } from "@qltysh/fabro-api-client";
import type { StageInferenceProjection } from "@qltysh/fabro-api-client";

import { Tooltip } from "./ui";
import { formatAbsoluteTs, formatDurationSecs } from "../lib/format";
import { elapsedSecsSince, useTickingNow } from "../lib/time";

export interface StageInferenceIndicatorProps {
  /** Open inference bracket from the stage projection, if there is one. */
  inference: StageInferenceProjection | null | undefined;
  /**
   * The run can no longer make progress on this bracket: it reached a terminal
   * status, or the stall watchdog fired. An open bracket then means *we never
   * learned how the request ended*, not *it is still working*, so the readout
   * goes static.
   */
  settled: boolean;
}

const ACTIVITY_LABEL: Record<LlmOutputKind, string> = {
  [LlmOutputKind.REASONING]: "reasoning",
  [LlmOutputKind.TEXT]: "writing",
  [LlmOutputKind.TOOL_CALL]: "calling tools",
};

/**
 * Live readout for an open model request.
 *
 * Says only what the event log proves. There is no progress bar, percentage,
 * or ETA, because no completion estimate exists; the elapsed clock counts
 * since the request opened rather than claiming the model is still working;
 * and "reasoning" appears only when the provider actually sent reasoning
 * output, never as a guess filling a gap in the log.
 */
export function StageInferenceIndicator({
  inference,
  settled,
}: StageInferenceIndicatorProps) {
  // Ticking is what distinguishes "we are still hearing from this request"
  // from "this is a record of one that never closed", so it stops the moment
  // the bracket can no longer advance.
  const now = useTickingNow(Boolean(inference) && !settled);

  if (!inference) return null;

  if (settled) {
    return (
      <p className="pb-2 text-xs text-fg-muted">
        Model request opened {formatAbsoluteTs(inference.started_at)}, never
        completed
      </p>
    );
  }

  const elapsedSecs = elapsedSecsSince(inference.started_at, now);
  const activity = inference.first_output_kind
    ? ACTIVITY_LABEL[inference.first_output_kind]
    : `waiting on ${inference.requested_model.model_id}`;

  const parts = ["Model request", activity];
  if (elapsedSecs !== null) parts.push(formatDurationSecs(elapsedSecs));
  // A retry that later succeeds is normal, so this is a count, not a failure.
  if (inference.retries > 0) parts.push(`retry ${inference.retries}`);

  return (
    <p className="pb-2 text-xs text-fg-muted" aria-live="polite">
      <Tooltip
        label={`Model request opened ${formatAbsoluteTs(inference.started_at)}`}
      >
        <span className="inline-flex items-center gap-1.5">
          <span
            className="size-1.5 animate-pulse rounded-full bg-teal-500"
            aria-hidden="true"
          />
          {parts.join(" · ")}
        </span>
      </Tooltip>
    </p>
  );
}
