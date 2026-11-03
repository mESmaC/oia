//! Synthetic captures establish byte preservation and honest coverage, not a
//! measured provider integration or full A18 acceptance.
use oia_analysis::snapshot_change::{
    Capture, ChangeError, ChangeRule, MAX_TEXT_BYTES, RetrievalFreshness, RuleDecision, Snapshot,
    SnapshotInput, SnapshotScope, StopReason, TextComparison, UnavailableReason, evaluate,
};
use oia_domain::{OpaqueId, VersionId};
use std::error::Error;
use std::time::{Duration, UNIX_EPOCH};

fn scope() -> Result<SnapshotScope, Box<dyn Error>> {
    Ok(SnapshotScope {
        workspace: OpaqueId::new("fixture-workspace")?,
        source: OpaqueId::new("fixture-source")?,
        resource: OpaqueId::new("fixture-page")?,
    })
}

fn snapshot<'a>(
    capture: Capture<'a>,
    version: &str,
    seconds: u64,
) -> Result<Snapshot<'a>, Box<dyn Error>> {
    Ok(Snapshot::new(SnapshotInput {
        scope: scope()?,
        version: VersionId::new(version)?,
        attribution: "Synthetic A18 page",
        retrieved_at: UNIX_EPOCH + Duration::from_secs(seconds),
        capture,
    })?)
}

#[test]
fn exact_spans_preserve_unicode_and_versions_without_promoting_missing_coverage()
-> Result<(), Box<dyn Error>> {
    let rule = ChangeRule::new(
        scope()?,
        VersionId::new("rule-v3")?,
        Duration::from_secs(10),
    )?;
    // A compact set catches UTF-8 byte boundaries, CRLF preservation, empty
    // captures and prefix/suffix overlap in insertion/deletion/repeated text.
    for (left, right) in [
        ("Station café\r\nopen\n", "Station café\r\nclosed\n"),
        ("é", "ê"),
        ("a🛰z", "a🌍z"),
        ("aba", "ababa"),
        ("abc", "ab"),
        ("", "雪"),
        ("雪", ""),
        ("", ""),
        ("same\r\n", "same\r\n"),
    ] {
        let before = snapshot(Capture::Complete(left), "capture-v1", 10)?;
        let after = snapshot(Capture::Complete(right), "capture-v2", 20)?;
        let result = evaluate(
            &rule,
            &before,
            &after,
            UNIX_EPOCH + Duration::from_secs(30),
            || Ok(()),
        )?;
        assert_eq!(result.rule().version().as_str(), "rule-v3");
        assert_eq!(result.before().input(), before.input());
        assert_eq!(result.after().input(), after.input());
        assert_eq!(result.baseline_retrieval_age(), Duration::from_secs(20));
        assert_eq!(
            result.baseline_retrieval_freshness(),
            RetrievalFreshness::Stale
        );
        assert_eq!(
            result.retrieval_freshness(),
            RetrievalFreshness::WithinRuleAge
        );
        match result.comparison() {
            TextComparison::Changed {
                before: old,
                after: new,
            } => {
                let mut reconstructed = left.to_owned();
                let replacement = right
                    .get(new.clone())
                    .ok_or("after span is not a UTF-8 boundary")?;
                assert!(left.get(old.clone()).is_some());
                reconstructed.replace_range(old.clone(), replacement);
                assert_eq!(reconstructed, right);
                assert_eq!(result.decision(), RuleDecision::ChangeDetected);
            }
            TextComparison::Unchanged => {
                assert_eq!(left, right);
                assert_eq!(result.decision(), RuleDecision::NoTextChange);
            }
            TextComparison::Inconclusive => {
                return Err("complete captures became inconclusive".into());
            }
        }
    }
    let before = snapshot(Capture::Complete("old"), "before", 10)?;
    let after = snapshot(Capture::Complete("new"), "after", 20)?;
    let stale = evaluate(
        &rule,
        &before,
        &after,
        UNIX_EPOCH + Duration::from_secs(31),
        || Ok(()),
    )?;
    assert_eq!(stale.decision(), RuleDecision::StaleCurrentCapture);
    assert!(matches!(stale.comparison(), TextComparison::Changed { .. }));
    for capture in [
        Capture::Partial {
            text: "",
            reason: "capture budget exhausted",
        },
        Capture::Unavailable(UnavailableReason::Restricted),
    ] {
        let incomplete = snapshot(capture, "incomplete", 20)?;
        let result = evaluate(
            &rule,
            &before,
            &incomplete,
            UNIX_EPOCH + Duration::from_secs(25),
            || Ok(()),
        )?;
        assert_eq!(result.comparison(), &TextComparison::Inconclusive);
        assert_eq!(result.decision(), RuleDecision::IncompleteCaptures);
        assert_eq!(result.after().input().capture, capture);
    }
    Ok(())
}

#[test]
fn invalid_scope_time_budget_and_cancellation_cannot_publish_a_comparison()
-> Result<(), Box<dyn Error>> {
    let rule = ChangeRule::new(
        scope()?,
        VersionId::new("rule-v1")?,
        Duration::from_secs(10),
    )?;
    let before = snapshot(Capture::Complete("a"), "before", 10)?;
    let after = snapshot(Capture::Complete("b"), "after", 20)?;
    let at = UNIX_EPOCH + Duration::from_secs(30);
    for field in ["workspace", "source", "resource"] {
        let mut input = after.input().clone();
        let foreign = OpaqueId::new("different")?;
        match field {
            "workspace" => input.scope.workspace = foreign,
            "source" => input.scope.source = foreign,
            _ => input.scope.resource = foreign,
        }
        let wrong_scope = Snapshot::new(input)?;
        assert_eq!(
            evaluate(&rule, &before, &wrong_scope, at, || Ok(())),
            Err(ChangeError::ScopeMismatch)
        );
    }
    assert_eq!(
        evaluate(&rule, &before, &before, at, || Ok(())),
        Err(ChangeError::ReusedSnapshotVersion)
    );
    assert_eq!(
        evaluate(&rule, &after, &before, at, || Ok(())),
        Err(ChangeError::ReversedRetrievalOrder)
    );
    assert_eq!(
        evaluate(&rule, &before, &after, UNIX_EPOCH, || Ok(())),
        Err(ChangeError::EvaluationBeforeRetrieval)
    );
    let oversized = "x".repeat(MAX_TEXT_BYTES + 1);
    assert!(snapshot(Capture::Complete(&oversized), "too-large", 10).is_err());
    let long_text = "x".repeat(8_192);
    let long_before = snapshot(Capture::Complete(&long_text), "long-before", 10)?;
    let long_after = snapshot(Capture::Complete(&long_text), "long-after", 20)?;
    let mut calls = 0;
    assert_eq!(
        evaluate(&rule, &long_before, &long_after, at, || {
            calls += 1;
            if calls == 3 {
                Err(StopReason::Cancelled)
            } else {
                Ok(())
            }
        }),
        Err(ChangeError::Stopped(StopReason::Cancelled))
    );
    // The final control check also fences a stop after the last text scan.
    let mut completed_calls = 0;
    evaluate(&rule, &before, &after, at, || {
        completed_calls += 1;
        Ok(())
    })?;
    let mut calls = 0;
    assert_eq!(
        evaluate(&rule, &before, &after, at, || {
            calls += 1;
            if calls == completed_calls {
                Err(StopReason::DeadlineExceeded)
            } else {
                Ok(())
            }
        }),
        Err(ChangeError::Stopped(StopReason::DeadlineExceeded))
    );
    Ok(())
}
