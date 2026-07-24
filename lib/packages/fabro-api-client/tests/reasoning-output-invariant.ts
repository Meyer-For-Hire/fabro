import type { ReasoningOutput } from "../src";

const summaryOnly: ReasoningOutput = {
  summary: "short summary",
};
const traceOnly: ReasoningOutput = {
  trace: "full trace",
};
const summaryAndTrace: ReasoningOutput = {
  summary: "short summary",
  trace: "full trace",
};

// @ts-expect-error ReasoningOutput requires at least one readable member.
const empty: ReasoningOutput = {};

void summaryOnly;
void traceOnly;
void summaryAndTrace;
void empty;
