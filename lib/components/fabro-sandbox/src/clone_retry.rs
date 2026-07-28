//! Retry for the first repository clone in a clone-based sandbox.
//!
//! Clone-based providers mint a GitHub installation access token and clone with
//! it in the same breath. GitHub replicates a new token to its edge cache sites
//! asynchronously, so a clone that starts within a second of the mint can be
//! rejected before the token is visible to the site serving it. On a private
//! repository that rejection arrives as `Repository not found.`, because GitHub
//! answers unauthorized reads with 404 rather than 403.
//!
//! A successful mint is what makes that message safe to retry.
//! `resolve_clone_credentials` already fails loudly on every deterministic
//! explanation for a clone 404: the installation lookup 404s when the App is
//! not installed for the owner, and token creation 422s when the installation
//! does not cover the repository. So once credentials are in hand, `not found`
//! from the clone itself cannot mean "no access" — the repository exists and
//! the token covers it.
//!
//! Retries reuse the same token on purpose. Replication of a given token only
//! makes progress, so each attempt strictly improves the odds, while re-minting
//! would restart the replication clock.

use std::future::Future;
use std::time::Duration;

use fabro_util::backoff::BackoffPolicy;
use tokio::time;

/// Total clone attempts, including the first.
const MAX_ATTEMPTS: u32 = 3;

/// Why a failed clone attempt is worth repeating.
#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::Display)]
#[strum(serialize_all = "snake_case")]
pub(crate) enum CloneRetryReason {
    /// A freshly minted installation token has not reached the GitHub edge
    /// cache site serving this clone yet.
    TokenReplication,
    /// The clone failed on infrastructure, unrelated to credentials.
    TransientInfra,
}

/// Message fragments that mean the clone failed on infrastructure.
///
/// These are safe to retry whether or not the clone was authenticated.
const TRANSIENT_HINTS: &[&str] = &[
    "could not resolve host",
    "temporary failure in name resolution",
    "connection refused",
    "connection reset",
    "connection timed out",
    "timed out",
    "network is unreachable",
    "no route to host",
    "tls handshake",
    "early eof",
    "rpc failed",
    "unexpected disconnect",
    "the remote end hung up unexpectedly",
    "index-pack failed",
    "service unavailable",
    "gateway timeout",
    "too many requests",
    "rate limit",
];

/// Message fragments GitHub uses when a token is not yet visible.
///
/// Only meaningful when the clone carried credentials. The same lag surfaces as
/// 404 or as an auth failure depending on which endpoint answers first.
const TOKEN_REPLICATION_HINTS: &[&str] = &[
    "repository not found",
    "authentication failed",
    "invalid username or password",
    "bad credentials",
];

/// Classify a failed clone by its rendered message.
///
/// `has_credentials` gates the token-replication reading. Without credentials
/// there is no token to replicate, so `not found` on a public clone means the
/// URL names a repository that is not there — that should fail immediately
/// rather than burn 12 seconds of backoff.
pub(crate) fn classify_message(message: &str, has_credentials: bool) -> Option<CloneRetryReason> {
    let lower = message.to_ascii_lowercase();

    if TRANSIENT_HINTS.iter().any(|hint| lower.contains(hint)) {
        return Some(CloneRetryReason::TransientInfra);
    }
    if has_credentials
        && TOKEN_REPLICATION_HINTS
            .iter()
            .any(|hint| lower.contains(hint))
    {
        return Some(CloneRetryReason::TokenReplication);
    }
    None
}

/// Backoff between clone attempts: 3s, then 9s.
///
/// GitHub's guidance for token replication is to wait a few seconds and retry
/// with the same token. Sub-second delays land inside the same replication
/// window and spend an attempt for nothing.
fn backoff() -> BackoffPolicy {
    BackoffPolicy {
        initial_delay: Duration::from_secs(3),
        factor:        3.0,
        max_delay:     Duration::from_secs(10),
        jitter:        false,
    }
}

/// Run a clone, repeating it while the failure looks transient.
///
/// `attempt` receives the 1-based attempt number so the caller can clear
/// leftovers from the previous try before cloning again. `classify` decides
/// whether an error is worth repeating; `None` returns it to the caller
/// untouched. The error from the final attempt is returned as-is, so callers
/// keep the cause chain they would have had without retries.
pub(crate) async fn retry_clone<T, E, Attempt, Fut, Classify>(
    provider: &'static str,
    mut attempt: Attempt,
    classify: Classify,
) -> Result<T, E>
where
    Attempt: FnMut(u32) -> Fut,
    Fut: Future<Output = Result<T, E>>,
    Classify: Fn(&E) -> Option<CloneRetryReason>,
{
    let backoff = backoff();

    for attempt_number in 1..MAX_ATTEMPTS {
        match attempt(attempt_number).await {
            Ok(value) => return Ok(value),
            Err(err) => {
                let Some(reason) = classify(&err) else {
                    return Err(err);
                };
                let delay = backoff.delay_for_attempt(attempt_number);
                // The failure text can carry git stderr, so log the category
                // rather than the message. The caller still reports the full
                // error if the attempts run out.
                tracing::warn!(
                    provider,
                    attempt = attempt_number,
                    max_attempts = MAX_ATTEMPTS,
                    reason = %reason,
                    delay_ms = u64::try_from(delay.as_millis()).unwrap_or(u64::MAX),
                    "Git clone failed, retrying"
                );
                time::sleep(delay).await;
            }
        }
    }

    attempt(MAX_ATTEMPTS).await
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// Records the attempt numbers a closure was called with.
    #[derive(Default)]
    struct Attempts(Mutex<Vec<u32>>);

    impl Attempts {
        fn record(&self, attempt: u32) {
            self.0.lock().expect("attempt log mutex").push(attempt);
        }

        fn recorded(&self) -> Vec<u32> {
            self.0.lock().expect("attempt log mutex").clone()
        }
    }

    /// A classifier that treats every failure as worth repeating.
    const ALWAYS_RETRY: fn(&String) -> Option<CloneRetryReason> =
        |_| Some(CloneRetryReason::TokenReplication);

    #[test]
    fn private_repo_not_found_after_a_successful_mint_is_a_replication_lag() {
        assert_eq!(
            classify_message("repository not found: Repository not found.", true),
            Some(CloneRetryReason::TokenReplication)
        );
    }

    #[test]
    fn not_found_without_credentials_is_a_wrong_url() {
        assert_eq!(
            classify_message("repository not found: Repository not found.", false),
            None
        );
    }

    #[test]
    fn auth_failure_with_credentials_is_a_replication_lag() {
        assert_eq!(
            classify_message(
                "fatal: Authentication failed for 'https://github.com/owner/repo'",
                true
            ),
            Some(CloneRetryReason::TokenReplication)
        );
    }

    #[test]
    fn infra_failures_retry_without_credentials() {
        for message in [
            "fatal: unable to access: Could not resolve host: github.com",
            "error: RPC failed; curl 56 recv failure",
            "fatal: early EOF",
            "Operation timed out",
        ] {
            assert_eq!(
                classify_message(message, false),
                Some(CloneRetryReason::TransientInfra),
                "expected {message:?} to be transient"
            );
        }
    }

    #[test]
    fn genuine_failures_are_not_retried() {
        for message in [
            "fatal: could not read Username for 'https://github.com'",
            "remote: Permission to owner/repo.git denied",
            "fatal: destination path 'repo' already exists",
        ] {
            assert_eq!(
                classify_message(message, true),
                None,
                "expected {message:?} to fail fast"
            );
        }
    }

    #[test]
    fn backoff_waits_seconds_not_milliseconds() {
        let backoff = backoff();
        assert_eq!(backoff.delay_for_attempt(1), Duration::from_secs(3));
        assert_eq!(backoff.delay_for_attempt(2), Duration::from_secs(9));
    }

    #[tokio::test(start_paused = true)]
    async fn first_success_runs_one_attempt() {
        let attempts = Attempts::default();

        let result = retry_clone(
            "test",
            |attempt| {
                attempts.record(attempt);
                async move { Ok::<_, String>(attempt) }
            },
            ALWAYS_RETRY,
        )
        .await;

        assert_eq!(result, Ok(1));
        assert_eq!(attempts.recorded(), vec![1]);
    }

    #[tokio::test(start_paused = true)]
    async fn retries_until_a_later_attempt_succeeds() {
        let attempts = Attempts::default();

        let result = retry_clone(
            "test",
            |attempt| {
                attempts.record(attempt);
                async move {
                    if attempt < 3 {
                        Err("Repository not found.".to_string())
                    } else {
                        Ok(attempt)
                    }
                }
            },
            ALWAYS_RETRY,
        )
        .await;

        assert_eq!(result, Ok(3));
        assert_eq!(attempts.recorded(), vec![1, 2, 3]);
    }

    #[tokio::test(start_paused = true)]
    async fn exhausted_attempts_return_the_final_error() {
        let attempts = Attempts::default();

        let result = retry_clone(
            "test",
            |attempt| {
                attempts.record(attempt);
                async move { Err::<(), _>(format!("Repository not found. (attempt {attempt})")) }
            },
            ALWAYS_RETRY,
        )
        .await;

        assert_eq!(
            result,
            Err("Repository not found. (attempt 3)".to_string()),
            "the caller should see the last failure, not the first"
        );
        assert_eq!(attempts.recorded(), vec![1, 2, 3]);
    }

    #[tokio::test(start_paused = true)]
    async fn unretryable_failure_stops_immediately() {
        let attempts = Attempts::default();

        let result = retry_clone(
            "test",
            |attempt| {
                attempts.record(attempt);
                async move { Err::<(), _>("permission denied".to_string()) }
            },
            |_: &String| None,
        )
        .await;

        assert_eq!(result, Err("permission denied".to_string()));
        assert_eq!(
            attempts.recorded(),
            vec![1],
            "a deterministic failure should not wait out the backoff"
        );
    }
}
