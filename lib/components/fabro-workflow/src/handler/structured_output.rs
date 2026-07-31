use std::sync::{Arc, LazyLock};

use fabro_graphviz::graph::Node;
use fabro_llm::types::{ResponseFormat, ResponseFormatType};
use jsonschema::Validator;
use serde_json::Value;

use crate::error::Error;
use crate::outcome::{FailureCategory, FailureDetail, Outcome, StageOutcome};

pub(crate) const ROUTING_KEYWORD: &str = "routing";

pub(crate) const ROUTING_STATUS_FIELDS: &[&str] = &[
    "preferred_next_label",
    "outcome",
    "failure_reason",
    "suggested_next_ids",
    "context_updates",
];

const QUOTED_ROUTING_STATUS_FIELDS: &[&str] = &[
    "\"preferred_next_label\"",
    "\"outcome\"",
    "\"failure_reason\"",
    "\"suggested_next_ids\"",
    "\"context_updates\"",
];

/// Parsed `output_schema` declaration with a precompiled validator so that
/// repair turns don't recompile the schema on every iteration.
#[derive(Debug, Clone)]
pub(crate) enum OutputSchemaKind {
    Routing,
    JsonSchema {
        schema:    Value,
        validator: Arc<Validator>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StructuredOutputErrorKind {
    NoJsonObject,
    NoRelevantJsonObject,
    InvalidJson,
    SchemaValidation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StructuredOutputError {
    kind:     StructuredOutputErrorKind,
    messages: Vec<String>,
}

impl StructuredOutputError {
    fn new(kind: StructuredOutputErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            messages: vec![message.into()],
        }
    }

    fn validation(messages: Vec<String>) -> Self {
        Self {
            kind: StructuredOutputErrorKind::SchemaValidation,
            messages,
        }
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn kind(&self) -> StructuredOutputErrorKind {
        self.kind
    }

    #[must_use]
    pub(crate) fn messages(&self) -> &[String] {
        &self.messages
    }

    #[must_use]
    pub(crate) fn allows_routing_fallback(&self) -> bool {
        matches!(
            self.kind,
            StructuredOutputErrorKind::NoJsonObject
                | StructuredOutputErrorKind::NoRelevantJsonObject
        )
    }

    #[must_use]
    pub(crate) fn repair_message(&self, schema: &OutputSchemaKind) -> String {
        let expectation = output_expectation(schema);
        let errors = self
            .messages
            .iter()
            .map(|message| format!("- {message}"))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "Your previous response did not satisfy the node's output_schema.\n\n\
             Validation errors:\n{errors}\n\n\
             {expectation}\n\
             Do not include Markdown fences or explanatory prose; reply only with the corrected JSON object."
        )
    }
}

#[must_use]
pub(crate) fn agent_prompt_with_output_schema(prompt: &str, schema: &OutputSchemaKind) -> String {
    let expectation = output_expectation(schema);
    format!(
        "{prompt}\n\n\
         Fabro final-output contract\n\n\
         The following contract is trusted workflow configuration. It applies only to your final response, not to intermediate tool calls.\n\
         {expectation}\n\
         The contract is complete. Do not ask the user to provide or choose the output shape."
    )
}

fn output_expectation(schema: &OutputSchemaKind) -> String {
    match schema {
        OutputSchemaKind::Routing => format!(
            "Return a single JSON object with at least one routing field: {}.",
            ROUTING_STATUS_FIELDS.join(", ")
        ),
        OutputSchemaKind::JsonSchema { schema, .. } => format!(
            "Return a single JSON object that satisfies this JSON Schema:\n\
             <output_schema>\n\
             {schema}\n\
             </output_schema>"
        ),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ValidatedStructuredOutput {
    pub(crate) value: Value,
}

#[must_use]
pub(crate) fn output_key(node_id: &str) -> String {
    format!("output.{node_id}")
}

#[must_use]
pub(crate) fn exhausted_failure_reason(repair_attempts: i64) -> String {
    format!("output schema validation failed after {repair_attempts} repair attempt(s)")
}

#[must_use]
pub(crate) fn exhausted_failure_outcome(repair_attempts: i64) -> Outcome {
    Outcome {
        status: StageOutcome::Failed {
            retry_requested: false,
        },
        failure: Some(FailureDetail::new(
            exhausted_failure_reason(repair_attempts),
            FailureCategory::Deterministic,
        )),
        ..Outcome::default()
    }
}

pub(crate) fn parse_node_output_schema(node: &Node) -> Result<Option<OutputSchemaKind>, Error> {
    let Some(raw) = node.output_schema() else {
        return Ok(None);
    };
    let value = raw.trim();
    if value.is_empty() {
        return Err(Error::Validation(format!(
            "Invalid output_schema for node \"{}\": value must not be empty",
            node.id
        )));
    }
    if value == ROUTING_KEYWORD {
        return Ok(Some(OutputSchemaKind::Routing));
    }
    if value.starts_with('@') {
        return Err(Error::Validation(format!(
            "Invalid output_schema for node \"{}\": unresolved file reference {value}",
            node.id
        )));
    }

    let schema = serde_json::from_str::<Value>(value).map_err(|err| {
        Error::Validation(format!(
            "Invalid output_schema for node \"{}\": expected \"routing\" or a JSON Schema object: {err}",
            node.id
        ))
    })?;
    let validator = jsonschema::validator_for(&schema).map_err(|err| {
        Error::Validation(format!(
            "Invalid output_schema for node \"{}\": {err}",
            node.id
        ))
    })?;
    Ok(Some(OutputSchemaKind::JsonSchema {
        schema,
        validator: Arc::new(validator),
    }))
}

#[must_use]
pub(crate) fn prompt_response_format(schema: &OutputSchemaKind) -> ResponseFormat {
    match schema {
        OutputSchemaKind::Routing => ResponseFormat {
            kind:        ResponseFormatType::JsonObject,
            json_schema: None,
            strict:      false,
        },
        OutputSchemaKind::JsonSchema { schema, .. } => ResponseFormat {
            kind:        ResponseFormatType::JsonSchema,
            json_schema: Some(schema.clone()),
            strict:      true,
        },
    }
}

pub(crate) fn validate_response_text(
    schema: &OutputSchemaKind,
    text: &str,
) -> Result<ValidatedStructuredOutput, StructuredOutputError> {
    match schema {
        OutputSchemaKind::Routing => validate_routing_response_text(text),
        OutputSchemaKind::JsonSchema { validator, .. } => {
            validate_custom_response_text(validator, text)
        }
    }
}

pub(crate) fn apply_validated_output(
    node: &Node,
    schema: &OutputSchemaKind,
    validated: &ValidatedStructuredOutput,
    outcome: &mut Outcome,
) {
    match schema {
        OutputSchemaKind::Routing => apply_routing_fields(&validated.value, outcome),
        OutputSchemaKind::JsonSchema { .. } => {
            outcome
                .context_updates
                .insert(output_key(&node.id), validated.value.clone());
        }
    }
}

/// Find the outermost balanced `{...}` JSON object substrings in the text, in
/// document order. Objects nested inside a match are skipped.
///
/// An unbalanced `{` does not suppress complete objects around or inside it:
/// the scan only skips ahead past a *matched* object, so it still walks into a
/// region that failed to close.
fn find_json_objects(text: &str) -> Vec<&str> {
    let mut results = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            let start = i;
            let mut depth = 0;
            let mut in_string = false;
            let mut escape = false;
            let mut j = i;
            while j < bytes.len() {
                let c = bytes[j];
                if escape {
                    escape = false;
                } else if c == b'\\' && in_string {
                    escape = true;
                } else if c == b'"' {
                    in_string = !in_string;
                } else if !in_string {
                    if c == b'{' {
                        depth += 1;
                    } else if c == b'}' {
                        depth -= 1;
                        if depth == 0 {
                            results.push(&text[start..=j]);
                            i = j;
                            break;
                        }
                    }
                }
                j += 1;
            }
        }
        i += 1;
    }
    results
}

/// Return the outermost balanced JSON object that ends the text, ignoring
/// trailing whitespace.
pub(crate) fn terminal_json_object(text: &str) -> Option<&str> {
    let trimmed = text.trim_end();
    find_json_objects(trimmed)
        .into_iter()
        .next_back()
        .filter(|candidate| trimmed.ends_with(candidate))
}

pub(crate) fn extract_status_fields(text: &str, outcome: &mut Outcome) -> bool {
    let candidates = find_json_objects(text);

    let parsed = candidates.iter().rev().find_map(|candidate| {
        let value: Value = serde_json::from_str(candidate).ok()?;
        if value.as_object().is_some_and(contains_routing_field) {
            Some(value)
        } else {
            None
        }
    });

    let Some(value) = parsed else { return false };
    apply_routing_fields(&value, outcome);
    true
}

fn validate_routing_response_text(
    text: &str,
) -> Result<ValidatedStructuredOutput, StructuredOutputError> {
    let candidates = find_json_objects(text);
    if candidates.is_empty() {
        return Err(StructuredOutputError::new(
            StructuredOutputErrorKind::NoJsonObject,
            "no JSON object found in response",
        ));
    }

    for candidate in candidates.iter().rev() {
        let parsed = match serde_json::from_str::<Value>(candidate) {
            Ok(value) => value,
            Err(err) if raw_mentions_routing_field(candidate) => {
                return Err(StructuredOutputError::new(
                    StructuredOutputErrorKind::InvalidJson,
                    format!("invalid routing JSON object: {err}"),
                ));
            }
            Err(_) => continue,
        };
        let Some(obj) = parsed.as_object() else {
            continue;
        };
        if !contains_routing_field(obj) {
            continue;
        }
        validate_value_against_validator(routing_validator(), &parsed)?;
        return Ok(ValidatedStructuredOutput { value: parsed });
    }

    Err(StructuredOutputError::new(
        StructuredOutputErrorKind::NoRelevantJsonObject,
        format!(
            "no JSON object contained any recognized routing field ({})",
            ROUTING_STATUS_FIELDS.join(", ")
        ),
    ))
}

fn validate_custom_response_text(
    validator: &Validator,
    text: &str,
) -> Result<ValidatedStructuredOutput, StructuredOutputError> {
    // Prose after the object can contain braces, so the last candidate is not
    // always JSON. Take the last one that parses; report its schema errors
    // rather than falling back to an earlier object that happens to validate.
    let candidates = find_json_objects(text);
    let mut invalid_json = None;
    for candidate in candidates.iter().rev() {
        match serde_json::from_str::<Value>(candidate) {
            Ok(parsed) => {
                validate_value_against_validator(validator, &parsed)?;
                return Ok(ValidatedStructuredOutput { value: parsed });
            }
            Err(err) if invalid_json.is_none() => invalid_json = Some(err.to_string()),
            Err(_) => {}
        }
    }

    Err(match invalid_json {
        Some(message) => StructuredOutputError::new(
            StructuredOutputErrorKind::InvalidJson,
            format!("invalid JSON object: {message}"),
        ),
        None => StructuredOutputError::new(
            StructuredOutputErrorKind::NoJsonObject,
            "no JSON object found in response",
        ),
    })
}

fn validate_value_against_validator(
    validator: &Validator,
    value: &Value,
) -> Result<(), StructuredOutputError> {
    let errors = validator
        .iter_errors(value)
        .map(|error| error.to_string())
        .take(5)
        .collect::<Vec<_>>();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(StructuredOutputError::validation(errors))
    }
}

fn contains_routing_field(obj: &serde_json::Map<String, Value>) -> bool {
    ROUTING_STATUS_FIELDS
        .iter()
        .any(|field| obj.contains_key(*field))
}

fn raw_mentions_routing_field(candidate: &str) -> bool {
    QUOTED_ROUTING_STATUS_FIELDS
        .iter()
        .any(|quoted_field| candidate.contains(quoted_field))
}

fn routing_validator() -> &'static Validator {
    static ROUTING_VALIDATOR: LazyLock<Validator> = LazyLock::new(|| {
        let schema = serde_json::json!({
            "type": "object",
            "additionalProperties": true,
            "properties": {
                "preferred_next_label": { "type": "string" },
                "outcome": {
                    "type": "string",
                    "enum": ["succeeded", "partially_succeeded", "failed", "skipped"]
                },
                "failure_reason": { "type": "string" },
                "suggested_next_ids": {
                    "type": "array",
                    "items": { "type": "string" }
                },
                "context_updates": { "type": "object" }
            },
            "anyOf": ROUTING_STATUS_FIELDS
                .iter()
                .map(|field| serde_json::json!({ "required": [field] }))
                .collect::<Vec<_>>()
        });
        jsonschema::validator_for(&schema).expect("built-in routing schema must compile")
    });
    &ROUTING_VALIDATOR
}

fn apply_routing_fields(value: &Value, outcome: &mut Outcome) {
    let Some(obj) = value.as_object() else {
        return;
    };

    if let Some(label) = obj.get("preferred_next_label").and_then(Value::as_str) {
        outcome.preferred_label = Some(label.to_string());
    }

    if let Some(ids) = obj.get("suggested_next_ids").and_then(Value::as_array) {
        let string_ids: Vec<String> = ids
            .iter()
            .filter_map(|value| value.as_str().map(String::from))
            .collect();
        if !string_ids.is_empty() {
            outcome.suggested_next_ids = string_ids;
        }
    }

    if let Some(status_str) = obj.get("outcome").and_then(Value::as_str) {
        if let Ok(status) = status_str.parse::<StageOutcome>() {
            outcome.status = status;
            if outcome.status.is_failure() {
                if let Some(reason) = obj.get("failure_reason").and_then(Value::as_str) {
                    outcome.failure =
                        Some(FailureDetail::new(reason, FailureCategory::Deterministic));
                }
            }
        }
    }

    if let Some(updates) = obj.get("context_updates").and_then(Value::as_object) {
        for (key, value) in updates {
            outcome.context_updates.insert(key.clone(), value.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use fabro_graphviz::graph::{AttrValue, Node};

    use super::*;

    fn routing() -> OutputSchemaKind {
        OutputSchemaKind::Routing
    }

    fn schema(value: Value) -> OutputSchemaKind {
        let validator =
            jsonschema::validator_for(&value).expect("test schema should be a valid JSON Schema");
        OutputSchemaKind::JsonSchema {
            schema:    value,
            validator: Arc::new(validator),
        }
    }

    /// A schema whose required field is itself an object, so validating the
    /// innermost `{...}` in the response would fail.
    fn issue_schema() -> OutputSchemaKind {
        schema(serde_json::json!({
            "type": "object",
            "required": ["issue"],
            "properties": {
                "issue": {
                    "type": "object",
                    "required": ["number"],
                    "properties": {
                        "number": { "type": "integer" }
                    }
                }
            }
        }))
    }

    #[test]
    fn validates_routing_json_and_applies_fields() {
        let validated = validate_response_text(
            &routing(),
            r#"done {"outcome":"failed","failure_reason":"tests failed","preferred_next_label":"fix","suggested_next_ids":["a"],"context_updates":{"verified":true}}"#,
        )
        .unwrap();
        let mut outcome = Outcome::success();

        apply_routing_fields(&validated.value, &mut outcome);

        assert_eq!(outcome.status, StageOutcome::Failed {
            retry_requested: false,
        });
        assert_eq!(
            outcome.failure.as_ref().map(|f| f.message.as_str()),
            Some("tests failed")
        );
        assert_eq!(outcome.preferred_label.as_deref(), Some("fix"));
        assert_eq!(outcome.suggested_next_ids, vec!["a".to_string()]);
        assert_eq!(
            outcome.context_updates.get("verified"),
            Some(&serde_json::json!(true)),
        );
    }

    #[test]
    fn routing_json_missing_routing_fields_is_invalid() {
        let error = validate_response_text(&routing(), r#"{"summary":"ok"}"#).unwrap_err();

        assert_eq!(
            error.kind(),
            StructuredOutputErrorKind::NoRelevantJsonObject
        );
        assert!(error.messages()[0].contains("recognized routing field"));
    }

    #[test]
    fn terminal_json_object_accepts_final_object_after_prose() {
        let object =
            terminal_json_object("# Results\n\n{\"context_updates\":{\"verified\":true}}\n\n");

        assert_eq!(object, Some(r#"{"context_updates":{"verified":true}}"#),);
    }

    #[test]
    fn terminal_json_object_returns_outermost_nested_object() {
        let object = terminal_json_object(r#"Results: {"context_updates":{"verified":true}}"#);

        assert_eq!(object, Some(r#"{"context_updates":{"verified":true}}"#),);
    }

    #[test]
    fn terminal_json_object_rejects_object_followed_by_content() {
        assert_eq!(
            terminal_json_object(
                "{\"outcome\":\"failed\",\"failure_reason\":\"tests failed\"}\nMore details",
            ),
            None,
        );
    }

    #[test]
    fn routing_json_with_wrong_field_type_is_invalid() {
        let error =
            validate_response_text(&routing(), r#"{"suggested_next_ids":[1]}"#).unwrap_err();

        assert_eq!(error.kind(), StructuredOutputErrorKind::SchemaValidation);
        assert!(
            error
                .messages()
                .iter()
                .any(|message| message.contains("string")),
            "unexpected messages: {:?}",
            error.messages(),
        );
    }

    #[test]
    fn validates_custom_schema_against_last_json_object() {
        let schema = schema(serde_json::json!({
            "type": "object",
            "required": ["passed"],
            "properties": {
                "passed": { "type": "boolean" }
            }
        }));

        let validated =
            validate_response_text(&schema, r#"ignore {"other":1} final {"passed":true}"#).unwrap();

        assert_eq!(validated.value, serde_json::json!({"passed": true}));
    }

    #[test]
    fn validates_custom_schema_against_outermost_object() {
        let validated =
            validate_response_text(&issue_schema(), r#"{"issue":{"number":19}}"#).unwrap();

        assert_eq!(
            validated.value,
            serde_json::json!({"issue": {"number": 19}})
        );
    }

    #[test]
    fn validates_last_outermost_object_when_response_has_trailing_prose() {
        let validated = validate_response_text(
            &issue_schema(),
            r#"ignore {"issue":{"number":1}} final {"issue":{"number":19}} trailing"#,
        )
        .unwrap();

        assert_eq!(
            validated.value,
            serde_json::json!({"issue": {"number": 19}})
        );
    }

    #[test]
    fn validates_last_parsable_object_when_trailing_prose_contains_braces() {
        let schema = schema(serde_json::json!({
            "type": "object",
            "required": ["passed"],
            "properties": {
                "passed": { "type": "boolean" }
            }
        }));

        let validated = validate_response_text(
            &schema,
            "{\"passed\":true}\n\nLet me know if {this works} for you.",
        )
        .unwrap();

        assert_eq!(validated.value, serde_json::json!({"passed": true}));
    }

    #[test]
    fn find_json_objects_returns_outermost_objects_only() {
        let cases = [
            (r#"{"a":{"b":1}}"#, vec![r#"{"a":{"b":1}}"#]),
            (r#"{"a":1} {"b":2}"#, vec![r#"{"a":1}"#, r#"{"b":2}"#]),
            (r#"{"a":1}{"b":2}"#, vec![r#"{"a":1}"#, r#"{"b":2}"#]),
            // An unclosed outer brace must not hide the complete object inside it.
            (r#"{ {"a":1}"#, vec![r#"{"a":1}"#]),
            (r#"{"a":1} {"#, vec![r#"{"a":1}"#]),
            // An unterminated string swallows the rest of its own candidate.
            (r#"{"a": "x} {"b":2}"#, vec![r#"{"b":2}"#]),
            // Braces inside strings are not delimiters.
            (r#"{"a":"} {"}"#, vec![r#"{"a":"} {"}"#]),
            ("no json here", vec![]),
        ];

        for (text, expected) in cases {
            assert_eq!(find_json_objects(text), expected, "input: {text}");
        }
    }

    #[test]
    fn custom_schema_validation_errors_are_reported() {
        let schema = schema(serde_json::json!({
            "type": "object",
            "required": ["passed"],
            "properties": {
                "passed": { "type": "boolean" }
            }
        }));

        let error = validate_response_text(&schema, r#"{"passed":"yes"}"#).unwrap_err();

        assert_eq!(error.kind(), StructuredOutputErrorKind::SchemaValidation);
        assert!(
            error
                .messages()
                .iter()
                .any(|message| message.contains("boolean")),
            "unexpected messages: {:?}",
            error.messages(),
        );
    }

    #[test]
    fn invalid_custom_schema_is_rejected_when_parsing_node_attr() {
        let mut node = Node::new("audit");
        node.attrs.insert(
            "output_schema".to_string(),
            AttrValue::String(r#"{"type": 5}"#.to_string()),
        );

        let error = parse_node_output_schema(&node).unwrap_err();

        assert!(
            error.to_string().contains("Invalid output_schema"),
            "unexpected error: {error}",
        );
    }

    #[test]
    fn invalid_json_candidate_is_reported_for_custom_schema() {
        let schema = schema(serde_json::json!({"type": "object"}));

        let error = validate_response_text(&schema, r"{not json}").unwrap_err();

        assert_eq!(error.kind(), StructuredOutputErrorKind::InvalidJson);
        assert!(error.messages()[0].contains("invalid JSON object"));
    }

    #[test]
    fn no_json_object_is_reported() {
        let error = validate_response_text(&routing(), "plain text only").unwrap_err();

        assert_eq!(error.kind(), StructuredOutputErrorKind::NoJsonObject);
        assert!(error.messages()[0].contains("no JSON object"));
    }

    #[test]
    fn parse_node_output_schema_accepts_builtin_routing_keyword() {
        let mut node = Node::new("route");
        node.attrs.insert(
            "output_schema".to_string(),
            AttrValue::String("routing".to_string()),
        );

        let parsed = parse_node_output_schema(&node).unwrap();

        assert!(matches!(parsed, Some(OutputSchemaKind::Routing)));
    }

    #[test]
    fn prompt_response_format_uses_json_schema_for_custom_schema() {
        let schema = schema(serde_json::json!({"type": "object"}));

        let format = prompt_response_format(&schema);

        assert_eq!(format.kind, ResponseFormatType::JsonSchema);
        assert_eq!(
            format.json_schema,
            Some(serde_json::json!({"type": "object"}))
        );
        assert!(format.strict);
    }

    #[test]
    fn apply_validated_custom_output_updates_output_context_key() {
        let node = Node::new("audit");
        let schema = schema(serde_json::json!({"type": "object"}));
        let validated = ValidatedStructuredOutput {
            value: serde_json::json!({"passed": true}),
        };
        let mut outcome = Outcome::success();

        apply_validated_output(&node, &schema, &validated, &mut outcome);

        assert_eq!(
            outcome.context_updates.get("output.audit"),
            Some(&serde_json::json!({"passed": true})),
        );
    }
}
