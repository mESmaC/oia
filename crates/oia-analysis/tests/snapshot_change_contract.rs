//! Synthetic captures establish byte preservation and honest coverage, not a
//! measured provider integration or full A18 acceptance.
use oia_analysis::snapshot_change::{
    Capture, CaptureCoverageSummary, ChangeError, ChangeExplanation, ChangeExplanationLimits,
    ChangeHunkKind, ChangeRule, EXPLANATION_METHOD_ID, MAX_TEXT_BYTES, RULE_CRITERION_ID,
    RetrievalFreshness, RuleDecision, Snapshot, SnapshotInput, SnapshotScope, StopReason,
    TextComparison, UnavailableReason, evaluate,
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
    // captures, multi-hunk edits, and prefix/suffix overlap in insertion/deletion/repeated text.
    for (left, right) in [
        ("Station café\r\nopen\n", "Station café\r\nclosed\n"),
        (
            "Header\nAlpha\nKeep\nOmega\nFooter\n",
            "Header\nAlpha-v2\nKeep\nOmega\nAdded\nFooter\n",
        ),
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
        let notification =
            result.explain_notification(ChangeExplanationLimits::default_limits(), || Ok(()))?;
        assert_eq!(result.rule().version().as_str(), "rule-v3");
        assert_eq!(notification.rule_version().as_str(), "rule-v3");
        assert_eq!(notification.rule_criterion(), RULE_CRITERION_ID);
        assert_eq!(notification.scope(), &scope()?);
        assert_eq!(notification.before_version().as_str(), "capture-v1");
        assert_eq!(notification.after_version().as_str(), "capture-v2");
        assert_eq!(notification.before_attribution(), "Synthetic A18 page");
        assert_eq!(notification.after_attribution(), "Synthetic A18 page");
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
                assert_eq!(notification.decision(), RuleDecision::ChangeDetected);

                // Verify line-level hunks reconstruct `right` from `left` exactly.
                match notification.explanation() {
                    ChangeExplanation::Changed {
                        method_id,
                        before_line_count,
                        after_line_count,
                        unchanged_line_count,
                        inserted_line_count,
                        removed_line_count,
                        coalesced_window_exceeded_lcs_budget,
                        hunks,
                    } => {
                        assert_eq!(*method_id, EXPLANATION_METHOD_ID);
                        assert!(!coalesced_window_exceeded_lcs_budget);
                        assert!(!hunks.is_empty());
                        assert_eq!(
                            *before_line_count,
                            unchanged_line_count + removed_line_count
                        );
                        assert_eq!(
                            *after_line_count,
                            unchanged_line_count + inserted_line_count
                        );
                        let mut by_hunks = left.to_owned();
                        for hunk in hunks.iter().rev() {
                            let rep = right
                                .get(hunk.after_bytes())
                                .ok_or("hunk after_bytes not UTF-8 boundary")?;
                            assert!(left.get(hunk.before_bytes()).is_some());
                            by_hunks.replace_range(hunk.before_bytes(), rep);
                        }
                        assert_eq!(by_hunks, right);
                    }
                    other => {
                        return Err(format!("expected Changed explanation, got {other:?}").into());
                    }
                }
            }
            TextComparison::Unchanged => {
                assert_eq!(left, right);
                assert_eq!(result.decision(), RuleDecision::NoTextChange);
                assert_eq!(
                    notification.explanation(),
                    &ChangeExplanation::Unchanged {
                        method_id: EXPLANATION_METHOD_ID,
                        line_count: left.split_inclusive('\n').count(),
                    }
                );
            }
            TextComparison::Inconclusive => {
                return Err("complete captures became inconclusive".into());
            }
        }
    }

    // Verify multi-hunk granularity explicitly: a modified line and a separate inserted line
    // separated by an unchanged line produce two distinct hunks.
    let multi_before = snapshot(
        Capture::Complete("Header\nAlpha\nKeep\nOmega\nFooter\n"),
        "multi-v1",
        10,
    )?;
    let multi_after = snapshot(
        Capture::Complete("Header\nAlpha-v2\nKeep\nOmega\nAdded\nFooter\n"),
        "multi-v2",
        20,
    )?;
    let multi_eval = evaluate(
        &rule,
        &multi_before,
        &multi_after,
        UNIX_EPOCH + Duration::from_secs(25),
        || Ok(()),
    )?;
    let multi_expl =
        multi_eval.explain_changes(ChangeExplanationLimits::default_limits(), || Ok(()))?;
    if let ChangeExplanation::Changed { hunks, .. } = multi_expl {
        assert_eq!(hunks.len(), 2);
        assert_eq!(hunks[0].kind(), ChangeHunkKind::Modified);
        assert_eq!(hunks[0].before_lines(), Some(2..=2));
        assert_eq!(hunks[0].after_lines(), Some(2..=2));
        assert_eq!(hunks[1].kind(), ChangeHunkKind::Inserted);
        assert_eq!(hunks[1].before_lines(), None);
        assert_eq!(hunks[1].after_lines(), Some(5..=5));
    } else {
        return Err("expected multi-hunk Changed explanation".into());
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
        let notif =
            result.explain_notification(ChangeExplanationLimits::default_limits(), || Ok(()))?;
        assert_eq!(notif.decision(), RuleDecision::IncompleteCaptures);
        assert!(matches!(
            notif.explanation(),
            ChangeExplanation::Inconclusive {
                baseline_coverage: CaptureCoverageSummary::Complete {
                    byte_len: 3,
                    line_count: 1
                },
                ..
            }
        ));
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

    // Verify explanation line/hunk bounds and cooperative cancellation.
    assert_eq!(
        ChangeExplanationLimits::new(0, 10),
        Err(ChangeError::InvalidExplanationLimits(
            "max_lines_per_capture"
        ))
    );
    assert_eq!(
        ChangeExplanationLimits::new(10, 0),
        Err(ChangeError::InvalidExplanationLimits("max_hunks"))
    );
    let multi_before = snapshot(
        Capture::Complete("L1\nChangeA\nKeep\nChangeB\nL5\n"),
        "b-hunk",
        10,
    )?;
    let multi_after = snapshot(
        Capture::Complete("L1\nChangeA2\nKeep\nChangeB2\nL5\n"),
        "a-hunk",
        20,
    )?;
    let multi_eval = evaluate(&rule, &multi_before, &multi_after, at, || Ok(()))?;
    assert_eq!(
        multi_eval.explain_changes(ChangeExplanationLimits::new(3, 10)?, || Ok(())),
        Err(ChangeError::LineLimitExceeded {
            actual_lines: 5,
            max_lines: 3,
        })
    );
    assert_eq!(
        multi_eval.explain_changes(ChangeExplanationLimits::new(10, 1)?, || Ok(())),
        Err(ChangeError::HunkLimitExceeded { max_hunks: 1 })
    );
    assert_eq!(
        multi_eval.explain_changes(ChangeExplanationLimits::default_limits(), || {
            Err(StopReason::Cancelled)
        }),
        Err(ChangeError::Stopped(StopReason::Cancelled))
    );
    Ok(())
}
