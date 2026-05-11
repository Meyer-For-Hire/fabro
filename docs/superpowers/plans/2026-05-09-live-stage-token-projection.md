# Live Stage Token Projection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `StageProjection` the source of truth for current per-stage token usage so in-flight stages show running token counts on the billing page.

**Architecture:** Store zeroable token counters directly on each `StageProjection`, plus optional `ModelRef` identity once model usage is known. Projection application updates those counters from usage-bearing events as they arrive, while terminal stage events replace live counts when they contain a later best-known billing snapshot. Billing rollups and server responses read from the projection rather than re-scanning raw events.

**Tech Stack:** Rust workspace crates `fabro-model`, `fabro-types`, `fabro-store`, `fabro-workflow`, `fabro-server`; OpenAPI-driven `fabro-api`; React route tests in `apps/fabro-web`.

---

## File Structure

- Modify `lib/crates/fabro-model/src/billing.rs` for OOP-style `BilledTokenCounts` aggregation helpers.
- Modify `lib/crates/fabro-types/src/run_projection.rs` for the new projection fields and attempt reset behavior.
- Modify `lib/crates/fabro-store/src/run_state.rs` so projection application updates live and terminal usage.
- Modify `lib/crates/fabro-workflow/src/billing_rollup.rs` and billing-related server code to read `StageProjection.usage`.
- Modify event conversion around existing `agent.message` events to carry typed `ModelRef` billing identity instead of a string model id.
- Modify `docs/public/api-reference/fabro-api.yaml` to expose `StageProjection.usage` and `StageProjection.model`.

## Task 1: Add `BilledTokenCounts` Aggregation Methods

**Files:**
- Modify: `lib/crates/fabro-model/src/billing.rs`

- [ ] Add methods to `impl BilledTokenCounts`:

```rust
pub fn add_counts(&mut self, source: &Self) {
    self.input_tokens += source.input_tokens;
    self.output_tokens += source.output_tokens;
    self.total_tokens += source.total_tokens;
    self.reasoning_tokens += source.reasoning_tokens;
    self.cache_read_tokens += source.cache_read_tokens;
    self.cache_write_tokens += source.cache_write_tokens;
    if let Some(value) = source.total_usd_micros {
        *self.total_usd_micros.get_or_insert(0) += value;
    }
}

pub fn add_billed_usage(&mut self, usage: &BilledModelUsage) {
    let tokens = usage.tokens();
    self.input_tokens += tokens.input_tokens;
    self.output_tokens += tokens.output_tokens;
    self.reasoning_tokens += tokens.reasoning_tokens;
    self.cache_read_tokens += tokens.cache_read_tokens;
    self.cache_write_tokens += tokens.cache_write_tokens;
    self.total_tokens += tokens.total_tokens();
    if let Some(value) = usage.total_usd_micros {
        *self.total_usd_micros.get_or_insert(0) += value;
    }
}

pub fn replace_with_billed_usage(&mut self, usage: &BilledModelUsage) {
    *self = Self::from_billed_usage(std::slice::from_ref(usage));
}

pub fn is_zero(&self) -> bool {
    self.input_tokens == 0
        && self.output_tokens == 0
        && self.total_tokens == 0
        && self.reasoning_tokens == 0
        && self.cache_read_tokens == 0
        && self.cache_write_tokens == 0
        && self.total_usd_micros.unwrap_or(0) == 0
}
```

- [ ] Add `fabro-model` unit tests covering `add_counts`, `add_billed_usage`, replacement, and unknown-cost behavior.

- [ ] Run:

```bash
cargo nextest run -p fabro-model billing
```

Expected: all `fabro-model` billing tests pass.

## Task 2: Make Stage Usage a First-Class Projection Field

**Files:**
- Modify: `lib/crates/fabro-types/src/run_projection.rs`
- Modify: `docs/public/api-reference/fabro-api.yaml`

- [ ] Change `StageProjection` from terminal-only internal usage to always-present counters:

```rust
#[serde(default)]
pub usage: BilledTokenCounts,
#[serde(default, skip_serializing_if = "Option::is_none")]
pub model: Option<ModelRef>,
```

- [ ] Remove the `#[serde(skip)] pub usage: Option<BilledModelUsage>` field.

- [ ] Initialize `usage` with `BilledTokenCounts::default()` and `model` with `None` in `StageProjection::new`.

- [ ] Confirm `StageProjection::begin_attempt` resets usage and model identity by relying on `*self = Self::new(self.first_event_seq)` before setting `started_at`, `handler`, and `state`.

- [ ] Update the OpenAPI `StageProjection` schema:
  - add `usage: $ref: "#/components/schemas/BilledTokenCounts"`
  - add `model: oneOf [$ref: "#/components/schemas/BillingModelRef", null]`
  - include `usage` in `required`

- [ ] Add an OpenAPI `BillingModelRef` schema for the existing Rust `fabro_types::ModelRef` shape:

```yaml
BillingModelRef:
  description: Provider-qualified billing model identity used for cost estimates.
  type: object
  required:
    - provider
    - model_id
  properties:
    provider:
      $ref: "#/components/schemas/Provider"
    model_id:
      type: string
    speed:
      oneOf:
        - $ref: "#/components/schemas/BillingSpeed"
        - type: "null"

BillingSpeed:
  description: Optional provider-specific model speed tier used for cost estimates.
  type: string
  enum:
    - standard
    - fast
```

- [ ] Do not reuse the existing OpenAPI `ModelRef` schema name here; that schema is a string parser type for run settings, while this field is the billing `fabro_types::ModelRef`.

- [ ] Run:

```bash
cargo build -p fabro-api
```

Expected: the generated Rust API crate still builds with `StageProjection` replaced by `fabro_types::StageProjection`.

## Task 3: Project Live and Terminal Usage in `fabro-store`

**Files:**
- Modify: `lib/crates/fabro-store/src/run_state.rs`

- [ ] Update `EventBody::AgentMessage` projection handling to add live usage deltas:

```rust
EventBody::AgentMessage(props) => {
    let Some(stage) = stage_at_stored_or_visit(self, stored, props.visit, event.seq) else {
        return Ok(());
    };
    stage.usage.add_counts(&props.billing);
    stage.model = Some(props.model.clone());
}
```

- [ ] Update `EventBody::PromptCompleted` to store the response and replace usage when `props.billing` is present:

```rust
stage.response = Some(props.response.clone());
if let Some(billing) = &props.billing {
    stage.usage.replace_with_billed_usage(billing);
    stage.model = Some(billing.model().clone());
}
```

- [ ] Update `EventBody::StageCompleted` and `EventBody::StageFailed` so terminal billing replaces live usage only when billing is present:

```rust
if let Some(billing) = &props.billing {
    stage.usage.replace_with_billed_usage(billing);
    stage.model = Some(billing.model().clone());
}
```

- [ ] Do not clear nonzero live usage on terminal events that have `billing: None`; this preserves best-known live usage for events that do not include final pricing.

- [ ] Update existing projection tests that assert `stage.usage.as_ref() == Some(...)` to assert flattened counters and `model`.

- [ ] Add tests for live `agent.message` accumulation, terminal replacement, `stage.failed` replacement, and reset on a new `stage.started`.

- [ ] Run:

```bash
cargo nextest run -p fabro-store run_state
```

Expected: projection tests pass.

## Task 4: Carry Typed Model Identity on `agent.message`

**Files:**
- Modify: `lib/crates/fabro-agent/src/types.rs`
- Modify: `lib/crates/fabro-agent/src/session.rs`
- Modify: `lib/crates/fabro-workflow/src/event/convert.rs`
- Modify: `lib/crates/fabro-types/src/run_event/agent.rs`

- [ ] Change the existing `AgentEvent::AssistantMessage` payload to carry billing `ModelRef` instead of a string model id:

```rust
AssistantMessage {
    text: String,
    model: ModelRef,
    usage: TokenCounts,
    tool_call_count: usize,
}
```

- [ ] Build the `ModelRef` at the source in `Session` when emitting `AgentEvent::AssistantMessage`:

```rust
let speed = self
    .config
    .speed
    .as_deref()
    .and_then(|value| value.parse::<Speed>().ok());
let model = ModelRef {
    provider: self.provider_profile.provider(),
    model_id: if response.model.is_empty() {
        self.provider_profile.model().to_string()
    } else {
        response.model.clone()
    },
    speed,
};
```

- [ ] Change `AgentMessageProps` to carry the same typed model identity:

```rust
pub struct AgentMessageProps {
    pub text:            String,
    pub model:           ModelRef,
    pub billing:         BilledTokenCounts,
    pub tool_call_count: usize,
    pub visit:           u32,
}
```

- [ ] In workflow event conversion, price assistant message usage directly from the typed model:

```rust
let requested_speed = model.speed.map(<&'static str>::from);
let billed = billed_model_usage_from_llm(
    &model.model_id,
    model.provider,
    requested_speed,
    usage,
);
let billing = BilledTokenCounts::from_billed_usage(std::slice::from_ref(&billed));
```

- [ ] Set `AgentMessageProps.model` from event conversion with `model.clone()` for every `agent.message`.

- [ ] Update affected unit tests that pattern-match `AgentEvent::AssistantMessage`.

- [ ] Run:

```bash
cargo nextest run -p fabro-agent
cargo nextest run -p fabro-workflow event::convert
```

Expected: agent event and event conversion tests pass.

## Task 5: Read Projection Usage in Billing Rollups and Server Responses

**Files:**
- Modify: `lib/crates/fabro-workflow/src/billing_rollup.rs`
- Modify: `lib/crates/fabro-server/src/server/handler/billing.rs`
- Modify: `lib/crates/fabro-server/src/server.rs`
- Modify: `lib/crates/fabro-server/src/server/handler/system.rs`

- [ ] Replace `stage.usage.is_some()` checks with `!stage.usage.is_zero()`.

- [ ] Change rollup model fields from string ids to billing model refs:

```rust
pub struct ProjectionBillingStage {
    pub node_id:     String,
    pub billing:     BilledTokenCounts,
    pub duration_ms: u64,
    pub model:       Option<ModelRef>,
}

pub struct ProjectionBillingByModel {
    pub model:   ModelRef,
    pub stages:  i64,
    pub billing: BilledTokenCounts,
}
```

- [ ] Group by `HashMap<ModelRef, ProjectionBillingByModel>` and sort the final `by_model` vector by `(provider.to_string(), model_id, speed)` before returning it, so tests and API responses stay deterministic without reducing model identity to a string key.

- [ ] Replace `if let Some(usage) = stage.usage.as_ref()` rollup logic with direct `BilledTokenCounts` aggregation:

```rust
if !stage.usage.is_zero() {
    billed_visit_count += 1;
    row.billing.add_counts(&stage.usage);
    totals.add_counts(&stage.usage);
    if let Some(model) = &stage.model {
        row.model = Some(model.clone());
        let model_entry = by_model.entry(model.clone()).or_insert_with(|| {
            ProjectionBillingByModel {
                model: model.clone(),
                stages: 0,
                billing: BilledTokenCounts::default(),
            }
        });
        model_entry.stages += 1;
        model_entry.billing.add_counts(&stage.usage);
    }
}
```

- [ ] Replace open-coded token count accumulation in server aggregate billing with `add_counts`.

- [ ] Keep public `/runs/{id}/billing` response shape unchanged for this plan: server handlers convert `ModelRef` to the existing `ModelReference { id: model.model_id.clone() }` response object. Do not add a new public model identity response type in this task.

- [ ] Keep runtime behavior unchanged: live runtime still comes from `started_at` and terminal runtime still comes from `duration_ms`.

- [ ] Run:

```bash
cargo nextest run -p fabro-workflow billing_rollup
cargo nextest run -p fabro-server run_billing
```

Expected: billing rollup and server billing tests pass.

## Task 6: Verify UI Behavior

**Files:**
- Modify tests only if the server response type changes generated client expectations.
- Likely tests: `apps/fabro-web/app/routes/run-billing.test.tsx`

- [ ] Confirm `RunBillingStage.billing` is still `BilledTokenCounts`, so the route should not need behavioral changes.

- [ ] Add or update a route test that renders an in-flight stage with nonzero `billing.input_tokens` and a ticking runtime.

- [ ] Run:

```bash
cd apps/fabro-web && bun test run-billing
```

Expected: billing route tests pass.

## Final Verification

- [ ] Run focused Rust checks:

```bash
cargo nextest run -p fabro-model
cargo nextest run -p fabro-store
cargo nextest run -p fabro-workflow billing
cargo nextest run -p fabro-server run_billing
```

- [ ] Run the API build after OpenAPI edits:

```bash
cargo build -p fabro-api
```

- [ ] Run the web route test:

```bash
cd apps/fabro-web && bun test run-billing
```

## Acceptance Criteria

- In-flight stages included in `/runs/{id}/billing` can show nonzero token counts before `stage.completed`.
- Completed stages replace live billing with terminal billing when terminal billing exists.
- Retry/new visit behavior resets per-visit usage cleanly.
- Billing totals, by-model totals, and stage rows are derived from `StageProjection`.
- Model identity uses the existing billing `ModelRef`; the plan does not introduce `StageModelIdentity` or store bare stage-level model-id strings.
