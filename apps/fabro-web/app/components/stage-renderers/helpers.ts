import { ReviewTargetKind, StageOutcome } from "@qltysh/fabro-api-client";
import type { EventEnvelope, ReviewTarget } from "@qltysh/fabro-api-client";

import {
  getArray,
  getNumber,
  getObject,
  getString,
  isRecord,
  type UnknownRecord,
} from "../../lib/unknown";

const STAGE_OUTCOMES: ReadonlySet<string> = new Set(Object.values(StageOutcome));

function asStageOutcome(value: string | undefined): StageOutcome | null {
  return value !== undefined && STAGE_OUTCOMES.has(value) ? (value as StageOutcome) : null;
}

export interface InterviewOption {
  key: string;
  label: string;
  description?: string | null;
  preview?: string | null;
}

export interface HumanQuestion {
  ts: string;
  questionId: string;
  question: string;
  questionType: string;
  options: InterviewOption[];
  allowFreeform: boolean;
  timeoutSeconds: number | null;
  contextDisplay: string | null;
  reviewTarget: ReviewTarget | null;
}

export type HumanResolution =
  | { kind: "answered"; ts: string; answer: string; durationMs: number; actor: string | null }
  | { kind: "timeout"; ts: string; durationMs: number }
  | { kind: "interrupted"; ts: string; reason: string; durationMs: number; actor: string | null };

export interface HumanInterviewPair {
  question: HumanQuestion;
  resolution: HumanResolution | null;
}

function principalLabel(actor: unknown): string | null {
  if (!actor || typeof actor !== "object") return null;
  const record = actor as UnknownRecord;
  const kind = getString(record, "kind") ?? "";
  if (kind === "user") {
    const email = getString(record, "email");
    const id = getString(record, "id");
    return email ?? id ?? "user";
  }
  if (kind === "worker") return "worker";
  if (kind === "webhook") return "webhook";
  if (kind === "slack") {
    const userId = getString(record, "user_id");
    return userId ? `slack:${userId}` : "slack";
  }
  return kind || null;
}

function parseInterviewOptions(value: unknown): InterviewOption[] {
  if (!Array.isArray(value)) return [];
  const out: InterviewOption[] = [];
  for (const item of value) {
    if (!item || typeof item !== "object") continue;
    const record = item as UnknownRecord;
    const key = getString(record, "key");
    const label = getString(record, "label");
    if (key && label) {
      const option: InterviewOption = { key, label };
      const description = getString(record, "description");
      const preview = getString(record, "preview");
      if (description !== null) option.description = description;
      if (preview !== null) option.preview = preview;
      out.push(option);
    }
  }
  return out;
}

function parseReviewTarget(value: unknown): ReviewTarget | null {
  if (!isRecord(value)) return null;
  const label = getString(value, "label");
  const url = getString(value, "url");
  const kind = getString(value, "kind");
  if (!label || !url || kind !== ReviewTargetKind.DOCUMENT) return null;
  return { label, url, kind };
}

/**
 * Pair `interview.started` events with the matching `interview.completed`,
 * `.timeout`, or `.interrupted` resolution by `question_id`. Unanswered
 * questions return with `resolution: null` so the UI can show pending state.
 */
export function parseHumanInterviewPairs(events: EventEnvelope[]): HumanInterviewPair[] {
  const pairs = new Map<string, HumanInterviewPair>();

  for (const event of events) {
    const props: UnknownRecord = event.properties ?? {};
    if (event.event === "interview.started") {
      const questionId = getString(props, "question_id");
      if (!questionId) continue;
      pairs.set(questionId, {
        question: {
          ts: event.ts,
          questionId,
          question: getString(props, "question") ?? "",
          questionType: getString(props, "question_type") ?? "freeform",
          options: parseInterviewOptions(props.options),
          allowFreeform: props.allow_freeform === true,
          timeoutSeconds: getNumber(props, "timeout_seconds") ?? null,
          contextDisplay: getString(props, "context_display") ?? null,
          reviewTarget: parseReviewTarget(props.review_target),
        },
        resolution: null,
      });
      continue;
    }
    if (event.event === "interview.completed") {
      const questionId = getString(props, "question_id");
      const pair = questionId ? pairs.get(questionId) : undefined;
      if (!pair) continue;
      pair.resolution = {
        kind: "answered",
        ts: event.ts,
        answer: getString(props, "answer") ?? "",
        durationMs: getNumber(props, "duration_ms") ?? 0,
        actor: principalLabel(props.actor),
      };
      continue;
    }
    if (event.event === "interview.timeout") {
      const questionId = getString(props, "question_id");
      const pair = questionId ? pairs.get(questionId) : undefined;
      if (!pair) continue;
      pair.resolution = {
        kind: "timeout",
        ts: event.ts,
        durationMs: getNumber(props, "duration_ms") ?? 0,
      };
      continue;
    }
    if (event.event === "interview.interrupted") {
      const questionId = getString(props, "question_id");
      const pair = questionId ? pairs.get(questionId) : undefined;
      if (!pair) continue;
      pair.resolution = {
        kind: "interrupted",
        ts: event.ts,
        reason: getString(props, "reason") ?? "interrupted",
        durationMs: getNumber(props, "duration_ms") ?? 0,
        actor: principalLabel(props.actor),
      };
    }
  }

  return Array.from(pairs.values()).sort((a, b) => a.question.ts.localeCompare(b.question.ts));
}

/** Identity and outcome of one branch, parsed from `parallel.completed`. */
export interface ParallelBranchSummary {
  id: string;
  index: number | null;
  itemLabel: string | null;
  status: StageOutcome;
}

export interface ParallelOverview {
  branchCount: number | null;
  results: ParallelBranchSummary[];
}

/**
 * Roll up the `parallel.started` (announces branch count) and
 * `parallel.completed` (carries the per-branch results) events for a parallel
 * stage. Pre-completion, only the announce data is available.
 *
 * Only branch identity is parsed. The event's own `success_count`,
 * `failure_count` and `duration_ms` rollups are deliberately ignored: the
 * renderer counts the branch rows it actually draws, and duration comes from
 * the stage record via `StageMetaBar`.
 */
export function parseParallelOverview(events: EventEnvelope[]): ParallelOverview {
  let branchCount: number | null = null;
  let results: ParallelBranchSummary[] = [];

  for (const event of events) {
    const props: UnknownRecord = event.properties ?? {};
    if (event.event === "parallel.started") {
      branchCount = getNumber(props, "branch_count") ?? branchCount;
    } else if (event.event === "parallel.completed") {
      const rawResults = getArray(props, "results") ?? [];
      results = rawResults
        .map((entry) => {
          const record = entry && typeof entry === "object" ? (entry as UnknownRecord) : null;
          if (!record) return null;
          const id = getString(record, "id");
          const index = getNumber(record, "index") ?? null;
          const itemLabel = getString(record, "item_label") ?? null;
          const status = asStageOutcome(getString(record, "status"));
          if (!id || !status) return null;
          return { id, index, itemLabel, status } satisfies ParallelBranchSummary;
        })
        .filter((r): r is ParallelBranchSummary => r != null);
      if (branchCount == null) branchCount = results.length;
    }
  }

  return { branchCount, results };
}

export interface ReducerTranscript {
  prompt: string;
  response: string;
  model: string | null;
  inputTokens: number;
  outputTokens: number;
}

/** Extract the standard prompt/response transcript emitted by an optional fan-in reducer. */
export function parseReducerTranscript(events: EventEnvelope[]): ReducerTranscript | null {
  let prompt = "";
  let response = "";
  let model: string | null = null;
  let inputTokens = 0;
  let outputTokens = 0;
  let hasReducer = false;

  for (const event of events) {
    const props: UnknownRecord = event.properties ?? {};
    if (event.event === "stage.prompt") {
      prompt = getString(props, "text") ?? prompt;
      model = getString(props, "model") ?? model;
      hasReducer = true;
    } else if (event.event === "prompt.completed" && hasReducer) {
      response = getString(props, "response") ?? response;
      model = getString(props, "model") ?? model;
      const billing = getObject(props, "billing") ?? {};
      inputTokens = getNumber(billing, "input_tokens") ?? inputTokens;
      outputTokens = getNumber(billing, "output_tokens") ?? outputTokens;
    }
  }

  return hasReducer ? { prompt, response, model, inputTokens, outputTokens } : null;
}

export interface StageContextData {
  routing: { preferredLabel: string | null; suggestedNextIds: string[] };
  /** `context_updates` keys the workflow deliberately set (engine keys removed). */
  updates: Record<string, unknown>;
}

// Engine/auto-populated context keys. These are bookkeeping or already shown in
// a stage's primary tab (command output, human answers, fan-in results), so the
// Context tab hides them and surfaces only what the workflow deliberately wrote.
const ENGINE_CONTEXT_KEYS = new Set(["last_stage", "last_response", "command.output"]);
const ENGINE_CONTEXT_PREFIXES = [
  "response.",
  "internal.",
  "current.",
  "human.gate.",
  "parallel.",
];

function isEngineContextKey(key: string): boolean {
  if (ENGINE_CONTEXT_KEYS.has(key)) return true;
  return ENGINE_CONTEXT_PREFIXES.some((prefix) => key.startsWith(prefix));
}

/**
 * Extract the workflow's deliberate outputs from the `stage.completed` event:
 * author-set `context_updates` (minus engine keys) plus the routing hints
 * (`preferred_label`, `suggested_next_ids`). Returns null when the stage hasn't
 * finished or produced nothing worth showing — which hides the Context tab.
 */
export function extractStageContext(events: EventEnvelope[]): StageContextData | null {
  for (const event of events) {
    if (event.event !== "stage.completed") continue;
    const props: UnknownRecord = event.properties ?? {};

    const rawUpdates = getObject(props, "context_updates") ?? {};
    const updates: Record<string, unknown> = {};
    for (const [key, value] of Object.entries(rawUpdates)) {
      if (!isEngineContextKey(key)) updates[key] = value;
    }

    const preferredLabel = getString(props, "preferred_label") ?? null;
    const suggestedNextIds = (getArray(props, "suggested_next_ids") ?? []).filter(
      (v): v is string => typeof v === "string",
    );

    if (
      Object.keys(updates).length === 0 &&
      !preferredLabel &&
      suggestedNextIds.length === 0
    ) {
      return null;
    }
    return { routing: { preferredLabel, suggestedNextIds }, updates };
  }
  return null;
}

export interface EdgeSelection {
  fromNode: string;
  toNode: string;
  reason: string;
  condition: string | null;
  isJump: boolean;
}

/**
 * Find the `edge.selected` event whose `from_node` matches this conditional
 * stage's node. Edge events are run-scoped (no stage_id) so callers must pass
 * the full run events list, not the per-stage events.
 *
 * When the stage runs multiple times, the most recent matching event wins —
 * for now we just take the last one. Sufficient until we surface visit data.
 */
export function findEdgeForNode(
  runEvents: EventEnvelope[],
  nodeId: string,
): EdgeSelection | null {
  let latest: EdgeSelection | null = null;
  for (const event of runEvents) {
    if (event.event !== "edge.selected") continue;
    const props = event.properties ?? {};
    const fromNode = getString(props, "from_node");
    if (fromNode !== nodeId) continue;
    const toNode = getString(props, "to_node") ?? "";
    latest = {
      fromNode: nodeId,
      toNode,
      reason: getString(props, "reason") ?? "",
      condition: getString(props, "condition") ?? null,
      isJump: props.is_jump === true,
    };
  }
  return latest;
}

// Re-export helper used by renderers that need to read nested properties.
export { getString };
