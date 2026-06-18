//! End-to-end coverage for **provider fallback routing reached through the real
//! `codel00p` binary**, fully hermetic (two scripted mock HTTP providers; no live
//! keys, no network).
//!
//! This supersedes the documented boundary in `tests/providers.rs`
//! (`fallback_routing_has_no_cli_surface`), which recorded that fallback routing
//! had *no* CLI/config/env surface. It now does: `agent run` accepts a repeatable
//! `--fallback <provider:model[@base_url]>` flag (and an `agent.fallbacks` config
//! list), parsed into [`codel00p_providers::InferenceFallbackRoute`]s and threaded
//! onto every inference request the turn issues. The providers layer already
//! consumed `fallback_routes` (`InferenceClient::complete`, proven at the library
//! layer in `codel00p-providers/tests/fallback_routing.rs`); the only thing
//! missing was the CLI/harness threading, which these tests exercise end-to-end.
//!
//! Both servers speak as the `custom` provider, so the single
//! `CODEL00P_PROVIDER_CUSTOM_API_KEY` the runner injects authenticates the primary
//! and the fallback alike. The runner wires the primary route via its standard
//! `--provider custom --model test-model --base-url <primary>` injection; the test
//! supplies the fallback as a `--fallback custom:test-model@<fallback>` flag.

use codel00p_e2e::{CodelRunner, MockProvider};

/// A rate-limited (`429`) primary route falls back to a healthy secondary, and
/// the run succeeds via the fallback. The primary *always* errors (so the
/// same-route retry is exhausted and fallback is taken), the fallback returns
/// final text, and the binary exits successfully having surfaced the fallback's
/// answer. Both endpoints are hit.
#[test]
fn rate_limited_primary_falls_back_to_a_healthy_secondary() {
    // Primary: every request to its single inference slot returns 429.
    let primary = MockProvider::start().always_error(429, "rate limit exceeded");
    // Fallback: returns final assistant text on its first inference slot.
    let fallback = MockProvider::start().assistant_text("answered via the fallback route");

    let runner = CodelRunner::new().with_provider(&primary);
    let fallback_route = format!("custom:test-model@{}", fallback.base_url());
    let result = runner.run(&[
        "agent",
        "run",
        "Say hi.",
        "--tool-set",
        "all",
        "--fallback",
        &fallback_route,
    ]);

    // The fallback-eligible 429 on the primary caused a transparent retry against
    // the configured fallback route, which succeeded — so the turn completed.
    result.assert_success();
    result.assert_turn_completed();

    // The fallback's answer is what surfaced.
    assert!(
        result.stdout().contains("answered via the fallback route"),
        "the fallback's assistant text should appear, stdout:\n{}",
        result.stdout()
    );

    // Both endpoints were exercised: the primary was hit (and kept failing) and
    // the fallback received the retried request.
    assert!(
        primary.hits() >= 1,
        "the primary route should have been tried, hits={}",
        primary.hits()
    );
    assert_eq!(
        fallback.hits(),
        1,
        "the fallback route should have served exactly one successful request, hits={}",
        fallback.hits()
    );
}

/// A `503`-class (provider-unavailable) primary failure is likewise
/// fallback-eligible: the binary routes to the fallback and succeeds. This
/// exercises the second fallback-eligible error family the classifier recognizes,
/// again through the real CLI inference path.
#[test]
fn unavailable_primary_falls_back_and_succeeds() {
    let primary = MockProvider::start().always_error(503, "service temporarily unavailable");
    let fallback = MockProvider::start().assistant_text("served by fallback after 503");

    let runner = CodelRunner::new().with_provider(&primary);
    let fallback_route = format!("custom:test-model@{}", fallback.base_url());
    let result = runner.run(&[
        "agent",
        "run",
        "Ping.",
        "--tool-set",
        "all",
        "--fallback",
        &fallback_route,
    ]);

    result.assert_success();
    assert!(
        result.stdout().contains("served by fallback after 503"),
        "the fallback's text should follow the 503, stdout:\n{}",
        result.stdout()
    );
    assert_eq!(
        fallback.hits(),
        1,
        "the fallback served exactly one request, hits={}",
        fallback.hits()
    );
}

/// A non-fallback-eligible primary failure (`401 unauthorized`) is **not** routed
/// to the fallback: the run fails and the fallback is never contacted. This is the
/// CLI-level mirror of the providers-layer
/// `does_not_fallback_for_non_fallbackable_errors` test, proving the threading
/// preserves the classifier's fallback eligibility rules end-to-end.
#[test]
fn non_fallbackable_primary_error_does_not_route_to_fallback() {
    let primary = MockProvider::start().always_error(401, "invalid api key");
    let fallback = MockProvider::start().assistant_text("should never run");

    let runner = CodelRunner::new().with_provider(&primary);
    let fallback_route = format!("custom:test-model@{}", fallback.base_url());
    let result = runner.run(&[
        "agent",
        "run",
        "Say hi.",
        "--tool-set",
        "all",
        "--fallback",
        &fallback_route,
    ]);

    // A 401 is auth-class (credential rotation), not fallback-eligible, so the run
    // fails outright and the fallback is never tried.
    assert!(
        !result.success(),
        "an auth-class primary error must not fall back; stdout:\n{}\nstderr:\n{}",
        result.stdout(),
        result.stderr()
    );
    assert_eq!(
        fallback.hits(),
        0,
        "the fallback must not be contacted for a non-fallbackable error, hits={}",
        fallback.hits()
    );
}
