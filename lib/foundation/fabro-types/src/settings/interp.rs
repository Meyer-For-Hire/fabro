//! Interpolation for config strings.
//!
//! An [`InterpString`] field may contain narrow `{{ <namespace>.NAME }}`
//! tokens — no template logic. Which [`Namespace`]s resolve is
//! scope-determined by the caller through [`ResolveCtx`]: run-scope settings
//! provide `vars` and `secrets`, a command node `script` provides `inputs`,
//! `vars`, and `goal`. A token whose namespace has no lookup in the resolution
//! context fails loudly rather than passing through as literal text.
//!
//! Two namespaces parse but never resolve. `inputs` and `goal` are bound only
//! where a run's values are in scope, which is the workflow graph rather than
//! general config. `env` resolves nowhere at all: the process environment is
//! not a configuration source, and `{{ vars.NAME }}` (non-sensitive, stored on
//! the server) or `{{ secrets.NAME }}` (vault-backed) replaces it. Keeping
//! them parseable is what lets an out-of-scope token fail with a message that
//! names the alternative instead of reaching a consumer as literal text.
//!
//! Resolution timing is split: `vars` and `inputs` substitute early
//! (server-side, at run creation) via [`InterpString::substitute_with`], while
//! `secrets` resolves late, at consumption time in the process that owns the
//! value, via [`InterpString::resolve_with`]. Resolved secret values are plain
//! strings; sensitivity is not tracked. Redaction of run output is
//! content-based (entropy analysis plus credential patterns), applied where
//! output is serialized.

use std::borrow::Cow;
use std::fmt;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::variable::is_env_style_name;

/// A config string that may contain `{{ env.NAME }}`, `{{ vars.NAME }}`,
/// `{{ secrets.NAME }}`, or `{{ inputs.NAME }}` tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterpString {
    segments: Vec<Segment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Literal(String),
    Token {
        namespace: Namespace,
        name:      String,
    },
}

/// The interpolation namespaces recognized inside `{{ ... }}` tokens.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, strum::Display, strum::EnumString, strum::IntoStaticStr,
)]
#[strum(serialize_all = "lowercase")]
pub enum Namespace {
    /// `{{ env.NAME }}` — the process environment. Parses so the token fails
    /// loudly, but resolves nowhere: use `vars` or `secrets` instead.
    Env,
    /// `{{ vars.NAME }}` — non-sensitive run variables, substituted early.
    Vars,
    /// `{{ secrets.NAME }}` — vault secrets, resolved at consumption time.
    Secrets,
    /// `{{ inputs.NAME }}` — workflow run inputs, substituted early.
    Inputs,
}

impl Namespace {
    /// The noun used for this namespace in error messages.
    fn noun(self) -> &'static str {
        match self {
            Self::Env => "environment variable",
            Self::Vars => "variable",
            Self::Secrets => "secret",
            Self::Inputs => "input",
        }
    }

    /// Parse a trimmed `{{ ... }}` token body into a namespace + name, or
    /// `None` when the body is not a recognized token (it then stays literal).
    fn parse_token(token: &str) -> Option<(Self, String)> {
        let trimmed = token.trim();
        let (prefix, name) = trimmed.split_once('.')?;
        let namespace = prefix.parse::<Self>().ok()?;
        namespace
            .is_valid_name(name)
            .then(|| (namespace, name.to_owned()))
    }

    fn is_valid_name(self, name: &str) -> bool {
        match self {
            // Preserves the original env token grammar: any non-empty run of
            // ASCII alphanumerics/underscores (leading digits allowed).
            Self::Env => {
                !name.is_empty()
                    && name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            }
            Self::Vars | Self::Secrets => is_env_style_name(name),
            // Input keys are TOML bare keys; additionally allow interior
            // hyphens.
            Self::Inputs => {
                let mut chars = name.chars();
                match chars.next() {
                    Some(first) if first.is_ascii_alphanumeric() || first == '_' => {}
                    _ => return false,
                }
                chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
            }
        }
    }
}

/// The namespace lookups available when resolving or substituting an
/// [`InterpString`].
///
/// Which namespaces are populated is scope-determined by the caller: a token
/// in a namespace with no lookup is a [`ResolveErrorKind::Unavailable`] error
/// under [`InterpString::resolve_with`], and passes through unchanged under
/// [`InterpString::substitute_with`].
#[derive(Default)]
pub struct ResolveCtx<'a> {
    vars:    Option<LookupFn<'a>>,
    secrets: Option<LookupFn<'a>>,
}

type LookupFn<'a> = Box<dyn FnMut(&str) -> Option<String> + 'a>;

impl<'a> ResolveCtx<'a> {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_vars(mut self, lookup: impl FnMut(&str) -> Option<String> + 'a) -> Self {
        self.vars = Some(Box::new(lookup));
        self
    }

    #[must_use]
    pub fn with_secrets(mut self, lookup: impl FnMut(&str) -> Option<String> + 'a) -> Self {
        self.secrets = Some(Box::new(lookup));
        self
    }

    fn lookup_for(&mut self, namespace: Namespace) -> Option<&mut LookupFn<'a>> {
        match namespace {
            Namespace::Vars => self.vars.as_mut(),
            Namespace::Secrets => self.secrets.as_mut(),
            // Neither is ever wired. `env` has no lookup because the process
            // environment is not a configuration source; `inputs` is
            // template-only. Both variants exist so the token fails with a
            // message naming where the value belongs, and `substitute_with`
            // still preserves them so a goal (an `InterpString` that feeds a
            // template) can forward `{{ inputs.* }}` to the template layer.
            Namespace::Env | Namespace::Inputs => None,
        }
    }
}

impl InterpString {
    fn push_literal(segments: &mut Vec<Segment>, text: &str) {
        if text.is_empty() {
            return;
        }

        match segments.last_mut() {
            Some(Segment::Literal(existing)) => existing.push_str(text),
            Some(Segment::Token { .. }) | None => {
                segments.push(Segment::Literal(text.to_owned()));
            }
        }
    }

    /// Parse a raw string into its literal/token segments.
    ///
    /// The [`From<String>`] and [`From<&str>`] impls delegate here.
    ///
    /// Parsing is infallible and intentionally permissive: only
    /// `{{ <known-namespace>.NAME }}` shaped tokens are claimed; any other
    /// `{{ ... }}` text (jq programs, Go templates, unterminated braces)
    /// stays literal. This is a documented known limitation — validation of
    /// claimed tokens happens at substitution/resolution time.
    #[must_use]
    pub fn parse(input: &str) -> Self {
        let mut segments: Vec<Segment> = Vec::new();
        let mut rest = input;

        while let Some(start) = rest.find("{{") {
            Self::push_literal(&mut segments, &rest[..start]);

            let after_open = &rest[start + 2..];
            if let Some(close) = after_open.find("}}") {
                let token = &after_open[..close];
                if let Some((namespace, name)) = Namespace::parse_token(token) {
                    segments.push(Segment::Token { namespace, name });
                } else {
                    Self::push_literal(&mut segments, &rest[start..start + 2 + close + 2]);
                }
                rest = &after_open[close + 2..];
            } else {
                // Unterminated token — treat the remainder as literal text.
                Self::push_literal(&mut segments, &rest[start..]);
                rest = "";
                break;
            }
        }

        if !rest.is_empty() {
            Self::push_literal(&mut segments, rest);
        }

        if segments.is_empty() {
            segments.push(Segment::Literal(String::new()));
        }

        Self { segments }
    }

    /// True when this string contains no interpolation tokens.
    #[must_use]
    pub fn is_literal(&self) -> bool {
        self.segments
            .iter()
            .all(|seg| matches!(seg, Segment::Literal(_)))
    }

    /// True when this string contains at least one token in `namespace`.
    #[must_use]
    pub fn references(&self, namespace: Namespace) -> bool {
        self.segments
            .iter()
            .any(|seg| matches!(seg, Segment::Token { namespace: ns, .. } if *ns == namespace))
    }

    /// The names referenced in `namespace` by this string, in source order.
    #[must_use]
    pub fn names(&self, namespace: Namespace) -> Vec<&str> {
        self.segments
            .iter()
            .filter_map(|seg| match seg {
                Segment::Token {
                    namespace: ns,
                    name,
                } if *ns == namespace => Some(name.as_str()),
                Segment::Literal(_) | Segment::Token { .. } => None,
            })
            .collect()
    }

    /// The raw, unresolved template source.
    ///
    /// This is a footgun for consumers: passing the raw source downstream
    /// leaks `{{ ... }}` tokens as literal text. Resolve via
    /// [`InterpString::resolve`] / [`InterpString::resolve_with`] (or
    /// substitute via [`InterpString::substitute_with`]) instead. Intentional
    /// uses — serialization, error messages, deliberate source preservation —
    /// must document themselves with
    /// `#[expect(clippy::disallowed_methods, reason = "...")]`.
    #[must_use]
    pub fn as_source(&self) -> String {
        let mut out = String::new();
        for seg in &self.segments {
            match seg {
                Segment::Literal(text) => out.push_str(text),
                Segment::Token { namespace, name } => {
                    out.push_str("{{ ");
                    out.push_str(namespace.into());
                    out.push('.');
                    out.push_str(name);
                    out.push_str(" }}");
                }
            }
        }
        out
    }

    /// Fully resolve every token using the lookups in `ctx`.
    ///
    /// Tokens in a namespace `ctx` has no lookup for fail with
    /// [`ResolveErrorKind::Unavailable`]: namespace availability is
    /// scope-determined, and a token outside its scope must fail loudly
    /// rather than pass through as literal text. A lookup miss fails with
    /// [`ResolveErrorKind::Missing`] — there is no fallback to the raw
    /// source.
    pub fn resolve_with(&self, ctx: &mut ResolveCtx<'_>) -> Result<String, ResolveError> {
        let mut value = String::new();
        for seg in &self.segments {
            match seg {
                Segment::Literal(text) => value.push_str(text),
                Segment::Token { namespace, name } => {
                    let Some(lookup) = ctx.lookup_for(*namespace) else {
                        return Err(ResolveError::unavailable(*namespace, name));
                    };
                    let Some(resolved) = lookup(name) else {
                        return Err(ResolveError::missing(*namespace, name));
                    };
                    value.push_str(&resolved);
                }
            }
        }

        Ok(value)
    }

    /// Substitute tokens for the namespaces `ctx` provides, preserving tokens
    /// for the namespaces it does not — their resolution happens later,
    /// possibly in a different process.
    ///
    /// This is the early, server-side pass (`vars`/`inputs`); late-bound
    /// namespaces (`env`/`secrets`) survive in token form for their
    /// consumption-time [`InterpString::resolve_with`].
    pub fn substitute_with(&self, ctx: &mut ResolveCtx<'_>) -> Result<Self, ResolveError> {
        let mut segments = Vec::new();
        for seg in &self.segments {
            match seg {
                Segment::Literal(text) => Self::push_literal(&mut segments, text),
                Segment::Token { namespace, name } => match ctx.lookup_for(*namespace) {
                    Some(lookup) => {
                        let Some(resolved) = lookup(name) else {
                            return Err(ResolveError::missing(*namespace, name));
                        };
                        Self::push_literal(&mut segments, &resolved);
                    }
                    None => segments.push(seg.clone()),
                },
            }
        }
        if segments.is_empty() {
            segments.push(Segment::Literal(String::new()));
        }
        Ok(Self { segments })
    }

    /// Substitute only `{{ vars.* }}` tokens while preserving all other
    /// namespaces for their consumption-time resolution.
    pub fn substitute_variables<F>(&self, lookup: F) -> Result<Self, ResolveError>
    where
        F: FnMut(&str) -> Option<String>,
    {
        self.substitute_with(&mut ResolveCtx::new().with_vars(lookup))
    }

    /// Substitute `{{ vars.* }}` tokens inside a plain string, returning the
    /// result in source form with all other tokens preserved.
    ///
    /// This is the string-typed counterpart of
    /// [`InterpString::substitute_variables`] for settings fields stored as
    /// `String`; it keeps the raw-source round-trip in one audited place.
    pub fn substitute_variables_in_str<F>(value: &str, lookup: F) -> Result<String, ResolveError>
    where
        F: FnMut(&str) -> Option<String>,
    {
        Self::substitute_variables_in_str_cow(value, lookup).map(Cow::into_owned)
    }

    pub(crate) fn substitute_variables_in_str_cow<F>(
        value: &str,
        lookup: F,
    ) -> Result<Cow<'_, str>, ResolveError>
    where
        F: FnMut(&str) -> Option<String>,
    {
        let parsed = Self::parse(value);
        if !parsed.references(Namespace::Vars) {
            return Ok(Cow::Borrowed(value));
        }
        #[expect(
            clippy::disallowed_methods,
            reason = "canonical raw-source round-trip for String-typed settings fields whose \
                      remaining tokens resolve downstream"
        )]
        Ok(Cow::Owned(parsed.substitute_variables(lookup)?.as_source()))
    }
}

impl From<String> for InterpString {
    fn from(value: String) -> Self {
        Self::parse(&value)
    }
}

impl From<&str> for InterpString {
    fn from(value: &str) -> Self {
        Self::parse(value)
    }
}

/// An error from resolving or substituting interpolation tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveError {
    pub namespace: Namespace,
    pub name:      String,
    pub kind:      ResolveErrorKind,
}

impl ResolveError {
    fn missing(namespace: Namespace, name: &str) -> Self {
        Self {
            namespace,
            name: name.to_string(),
            kind: ResolveErrorKind::Missing,
        }
    }

    fn unavailable(namespace: Namespace, name: &str) -> Self {
        Self {
            namespace,
            name: name.to_string(),
            kind: ResolveErrorKind::Unavailable,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveErrorKind {
    /// The namespace is available in this context but has no value for the
    /// referenced name.
    Missing,
    /// The namespace is not available in this resolution context.
    Unavailable,
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let noun = self.namespace.noun();
        let namespace = self.namespace;
        match self.kind {
            ResolveErrorKind::Missing => write!(
                f,
                "{noun} {:?} referenced by {{{{ {namespace}.{} }}}} is not set",
                self.name, self.name
            ),
            ResolveErrorKind::Unavailable => match namespace {
                // `inputs` is template-only: it never resolves in an
                // `InterpString` field. Point the user at where it works.
                Namespace::Inputs => write!(
                    f,
                    "{{{{ inputs.{} }}}} is only available in prompts and goals, not in other \
                     config fields",
                    self.name
                ),
                // `env` resolves nowhere. Name the replacement rather than
                // reporting a generic out-of-scope error.
                Namespace::Env => write!(
                    f,
                    "{{{{ env.{} }}}} is not supported: the process environment is not a \
                     configuration source. Use {{{{ vars.{} }}}} for a non-sensitive value \
                     (`fabro variable set`) or {{{{ secrets.{} }}}} for a credential \
                     (`fabro secret set`)",
                    self.name, self.name, self.name
                ),
                _ => write!(
                    f,
                    "{noun} {:?} referenced by {{{{ {namespace}.{} }}}} is not supported in \
                     this interpolation context",
                    self.name, self.name
                ),
            },
        }
    }
}

impl std::error::Error for ResolveError {}

impl Serialize for InterpString {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[expect(
            clippy::disallowed_methods,
            reason = "serialization round-trips the unresolved template source by design"
        )]
        serializer.serialize_str(&self.as_source())
    }
}

impl<'de> Deserialize<'de> for InterpString {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct InterpStringVisitor;

        impl Visitor<'_> for InterpStringVisitor {
            type Value = InterpString;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(
                    "a string, optionally containing {{ env.NAME }}, {{ vars.NAME }}, \
                     {{ secrets.NAME }}, or {{ inputs.NAME }} interpolation tokens",
                )
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<InterpString, E> {
                Ok(InterpString::parse(value))
            }

            fn visit_string<E: de::Error>(self, value: String) -> Result<InterpString, E> {
                Ok(InterpString::parse(&value))
            }
        }

        deserializer.deserialize_str(InterpStringVisitor)
    }
}

#[cfg(test)]
#[expect(
    clippy::disallowed_methods,
    reason = "tests assert raw template source round-trips"
)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn lookup_from(values: &[(&str, &str)]) -> impl FnMut(&str) -> Option<String> + 'static {
        let map: HashMap<String, String> = values
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |name| map.get(name).cloned()
    }

    #[test]
    fn literal_string_has_no_refs() {
        let s = InterpString::parse("hello world");
        assert!(s.is_literal());
        assert!(!s.references(Namespace::Env));
        assert_eq!(s.names(Namespace::Env), Vec::<&str>::new());
    }

    #[test]
    fn whole_value_env_reference() {
        let s = InterpString::parse("{{ env.API_KEY }}");
        assert!(!s.is_literal());
        assert_eq!(s.names(Namespace::Env), vec!["API_KEY"]);
        assert_eq!(s.as_source(), "{{ env.API_KEY }}");
    }

    #[test]
    fn substring_env_reference() {
        let s = InterpString::parse("Bearer {{ env.TOKEN }}");
        assert_eq!(s.names(Namespace::Env), vec!["TOKEN"]);
    }

    #[test]
    fn multi_token_env_reference() {
        let s = InterpString::parse("{{ env.USER }}@{{ env.HOST }}:{{env.PORT}}");
        assert_eq!(s.names(Namespace::Env), vec!["USER", "HOST", "PORT"]);
    }

    fn resolve_vars(s: &InterpString, pairs: &[(&str, &str)]) -> Result<String, ResolveError> {
        s.resolve_with(&mut ResolveCtx::new().with_vars(lookup_from(pairs)))
    }

    #[test]
    fn resolve_literal_string() {
        let s = InterpString::parse("static");
        assert_eq!(resolve_vars(&s, &[]).unwrap(), "static");
    }

    #[test]
    fn resolve_whole_value() {
        let s = InterpString::parse("{{ vars.API_KEY }}");
        assert_eq!(
            resolve_vars(&s, &[("API_KEY", "secret-123")]).unwrap(),
            "secret-123"
        );
    }

    #[test]
    fn resolve_substring() {
        let s = InterpString::parse("Bearer {{ vars.TOKEN }}");
        assert_eq!(resolve_vars(&s, &[("TOKEN", "abc")]).unwrap(), "Bearer abc");
    }

    #[test]
    fn resolve_multiple_tokens() {
        let s = InterpString::parse("{{ vars.USER }}@{{ vars.HOST }}");
        assert_eq!(
            resolve_vars(&s, &[("USER", "root"), ("HOST", "example.com")]).unwrap(),
            "root@example.com"
        );
    }

    #[test]
    fn resolve_missing_var_fails_with_name() {
        let s = InterpString::parse("{{ vars.MISSING }}");
        let err = resolve_vars(&s, &[]).unwrap_err();
        assert_eq!(err.name, "MISSING");
        assert_eq!(err.namespace, Namespace::Vars);
        assert_eq!(err.kind, ResolveErrorKind::Missing);
    }

    /// `env` still parses so the token fails loudly, but it resolves nowhere
    /// and the message names its replacements.
    #[test]
    fn env_token_parses_but_never_resolves() {
        let s = InterpString::parse("{{ env.API_KEY }}");
        let err = resolve_vars(&s, &[("API_KEY", "ignored")]).unwrap_err();

        assert_eq!(err.namespace, Namespace::Env);
        assert_eq!(err.kind, ResolveErrorKind::Unavailable);
        let message = err.to_string();
        assert!(message.contains("vars.API_KEY"), "{message}");
        assert!(message.contains("secrets.API_KEY"), "{message}");
    }

    #[test]
    fn unterminated_token_treated_as_literal() {
        let s = InterpString::parse("{{ env.OPEN");
        assert_eq!(resolve_vars(&s, &[]).unwrap(), "{{ env.OPEN");
    }

    #[test]
    fn unknown_namespace_token_stays_literal() {
        for raw in [
            "{{ unknown.NAME }}",
            "{{ .leading }}",
            "{{ env. }}",
            "{{ no_dot }}",
            "{{ secrets.bad-name }}",
            "{{ if .Values.foo }}",
        ] {
            let s = InterpString::parse(raw);
            assert!(s.is_literal(), "{raw} should stay literal");
            assert_eq!(resolve_vars(&s, &[]).unwrap(), raw);
        }
    }

    #[test]
    fn serde_round_trip_preserves_token_form() {
        #[derive(Debug, serde::Deserialize, serde::Serialize, PartialEq)]
        struct Wrap {
            s: InterpString,
        }

        let input = r#"{"s":"Bearer {{ env.TOKEN }}"}"#;
        let parsed: Wrap = serde_json::from_str(input).unwrap();
        assert_eq!(parsed.s.as_source(), "Bearer {{ env.TOKEN }}");
        let rendered = serde_json::to_string(&parsed).unwrap();
        assert_eq!(rendered, input);
    }

    #[test]
    fn serde_round_trip_preserves_all_namespaces() {
        #[derive(Debug, serde::Deserialize, serde::Serialize, PartialEq)]
        struct Wrap {
            s: InterpString,
        }

        let input = r#"{"s":"{{ env.A }}/{{ vars.B }}/{{ secrets.C }}/{{ inputs.d-key }}"}"#;
        let parsed: Wrap = serde_json::from_str(input).unwrap();
        let rendered = serde_json::to_string(&parsed).unwrap();
        assert_eq!(rendered, input);
    }

    #[test]
    fn vars_reference_round_trips_source() {
        let s = InterpString::parse("{{ vars.RUNTIME_TOKEN }}");

        assert_eq!(s.names(Namespace::Vars), vec!["RUNTIME_TOKEN"]);
        assert_eq!(s.as_source(), "{{ vars.RUNTIME_TOKEN }}");
    }

    #[test]
    fn resolve_with_substitutes_secret_and_var_tokens() {
        let s = InterpString::parse("https://{{ vars.REGION }}.{{ secrets.DOMAIN }}");

        let resolved = s
            .resolve_with(
                &mut ResolveCtx::new()
                    .with_vars(lookup_from(&[("REGION", "us-east-1")]))
                    .with_secrets(lookup_from(&[("DOMAIN", "example.com")])),
            )
            .unwrap();

        assert_eq!(resolved, "https://us-east-1.example.com");
    }

    #[test]
    fn resolve_with_reports_missing_variable() {
        let s = InterpString::parse("{{ vars.MISSING }}");

        let err = s
            .resolve_with(&mut ResolveCtx::new().with_vars(lookup_from(&[])))
            .unwrap_err();

        assert_eq!(err.name, "MISSING");
        assert_eq!(err.namespace, Namespace::Vars);
        assert_eq!(err.kind, ResolveErrorKind::Missing);
        assert_eq!(
            err.to_string(),
            "variable \"MISSING\" referenced by {{ vars.MISSING }} is not set"
        );
    }

    #[test]
    fn empty_context_rejects_vars_reference() {
        let s = InterpString::parse("{{ vars.RUNTIME_TOKEN }}");

        let err = s.resolve_with(&mut ResolveCtx::new()).unwrap_err();

        assert_eq!(err.name, "RUNTIME_TOKEN");
        assert_eq!(err.namespace, Namespace::Vars);
        assert_eq!(err.kind, ResolveErrorKind::Unavailable);
        assert_eq!(
            err.to_string(),
            "variable \"RUNTIME_TOKEN\" referenced by {{ vars.RUNTIME_TOKEN }} is not supported \
             in this interpolation context"
        );
    }

    #[test]
    fn empty_context_rejects_secrets_reference() {
        let s = InterpString::parse("{{ secrets.API_KEY }}");

        let err = s.resolve_with(&mut ResolveCtx::new()).unwrap_err();

        assert_eq!(err.namespace, Namespace::Secrets);
        assert_eq!(err.kind, ResolveErrorKind::Unavailable);
        assert_eq!(
            err.to_string(),
            "secret \"API_KEY\" referenced by {{ secrets.API_KEY }} is not supported in this \
             interpolation context"
        );
    }

    #[test]
    fn resolve_with_substitutes_secrets_and_vars() {
        let s = InterpString::parse("Bearer {{ secrets.API_KEY }} via {{ vars.PROXY }}");

        let resolved = s
            .resolve_with(
                &mut ResolveCtx::new()
                    .with_vars(lookup_from(&[("PROXY", "proxy.internal")]))
                    .with_secrets(lookup_from(&[("API_KEY", "vault-value")])),
            )
            .unwrap();

        assert_eq!(resolved, "Bearer vault-value via proxy.internal");
    }

    #[test]
    fn resolve_with_rejects_inputs_as_template_only() {
        // `inputs` is template-only. An `{{ inputs.* }}` token never resolves
        // in an `InterpString` field — it fails loudly, pointing the
        // user at prompts and goals.
        let s = InterpString::parse("run-{{ inputs.ticket-id }}");

        let err = s.resolve_with(&mut ResolveCtx::new()).unwrap_err();

        assert_eq!(err.namespace, Namespace::Inputs);
        assert_eq!(err.kind, ResolveErrorKind::Unavailable);
        assert!(
            err.to_string()
                .contains("only available in prompts and goals"),
            "unexpected message: {err}"
        );
    }

    #[test]
    fn substitute_variables_preserves_late_bound_tokens() {
        let s =
            InterpString::parse("{{ vars.NAME }}:{{ env.HOME }}:{{ secrets.KEY }}:{{ inputs.id }}");

        let substituted = s
            .substitute_variables(lookup_from(&[("NAME", "fabro")]))
            .unwrap();

        assert_eq!(
            substituted.as_source(),
            "fabro:{{ env.HOME }}:{{ secrets.KEY }}:{{ inputs.id }}"
        );
    }

    #[test]
    fn substitute_variables_reports_missing_variable() {
        let s = InterpString::parse("{{ vars.MISSING }}");

        let err = s.substitute_variables(lookup_from(&[])).unwrap_err();

        assert_eq!(err.namespace, Namespace::Vars);
        assert_eq!(err.kind, ResolveErrorKind::Missing);
    }

    #[test]
    fn substitute_with_merges_adjacent_literals() {
        let s = InterpString::parse("a{{ vars.B }}c");

        let substituted = s
            .substitute_with(&mut ResolveCtx::new().with_vars(lookup_from(&[("B", "b")])))
            .unwrap();

        assert!(substituted.is_literal());
        assert_eq!(substituted.as_source(), "abc");
    }

    #[test]
    fn substitute_variables_in_str_round_trips_source() {
        let out = InterpString::substitute_variables_in_str(
            "{{ vars.NAME }} at {{ env.HOME }}",
            lookup_from(&[("NAME", "fabro")]),
        )
        .unwrap();

        assert_eq!(out, "fabro at {{ env.HOME }}");
    }

    #[test]
    fn namespace_displays_lowercase() {
        assert_eq!(Namespace::Env.to_string(), "env");
        assert_eq!(Namespace::Vars.to_string(), "vars");
        assert_eq!(Namespace::Secrets.to_string(), "secrets");
        assert_eq!(Namespace::Inputs.to_string(), "inputs");
    }
}
