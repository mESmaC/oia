//! Synthetic captures establish byte preservation and honest coverage, not a
//! measured provider integration or full A18 acceptance.
use oia_analysis::snapshot_change::{
    Capture, CaptureCoverageSummary, ChangeError, ChangeExplanation, ChangeExplanationLimits,
    ChangeHunkKind, ChangeRule, CoalescedReason, EXPLANATION_METHOD_ID,
    ExplanationUnavailableReason, HunkAlignment, LineChangeCounts, MAX_TEXT_BYTES,
    RULE_CRITERION_ID, RetrievalFreshness, RuleDecision, Snapshot, SnapshotInput, SnapshotScope,
    StopReason, TextComparison, UnavailableReason, evaluate,
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
        ("a\r\nb\n", "a\nb\n"),
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

                // Verify line-level hunks reconstruct `right` from `left` and `left` from `right` byte-for-byte.
                match notification.explanation() {
                    ChangeExplanation::Changed {
                        method_id,
                        before_line_count,
                        after_line_count,
                        line_counts,
                        hunks,
                    } => {
                        assert_eq!(*method_id, EXPLANATION_METHOD_ID);
                        assert!(!hunks.is_empty());
                        match *line_counts {
                            LineChangeCounts::Exact {
                                unchanged_lines,
                                inserted_lines,
                                removed_lines,
                            } => {
                                assert_eq!(*before_line_count, unchanged_lines + removed_lines);
                                assert_eq!(*after_line_count, unchanged_lines + inserted_lines);
                            }
                            other => {
                                return Err(format!(
                                    "small synthetic pair unexpectedly used {other:?}"
                                )
                                .into());
                            }
                        }
                        let mut by_hunks = left.to_owned();
                        let mut reverse_by_hunks = right.to_owned();
                        for hunk in hunks.iter().rev() {
                            assert_eq!(hunk.alignment(), HunkAlignment::ExactMinimal);
                            let rep = right
                                .get(hunk.after_bytes())
                                .ok_or("hunk after_bytes not UTF-8 boundary")?;
                            let orig = left
                                .get(hunk.before_bytes())
                                .ok_or("hunk before_bytes not UTF-8 boundary")?;
                            by_hunks.replace_range(hunk.before_bytes(), rep);
                            reverse_by_hunks.replace_range(hunk.after_bytes(), orig);
                        }
                        assert_eq!(by_hunks, right);
                        assert_eq!(reverse_by_hunks, left);
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

    // Verify a line-ending-only change produces a Modified hunk on that line.
    let crlf_before = snapshot(Capture::Complete("a\r\nb\n"), "crlf-v1", 10)?;
    let lf_after = snapshot(Capture::Complete("a\nb\n"), "lf-v2", 20)?;
    let eol_eval = evaluate(
        &rule,
        &crlf_before,
        &lf_after,
        UNIX_EPOCH + Duration::from_secs(25),
        || Ok(()),
    )?;
    if let ChangeExplanation::Changed {
        line_counts, hunks, ..
    } = eol_eval.explain_changes(ChangeExplanationLimits::default_limits(), || Ok(()))?
    {
        assert_eq!(
            line_counts,
            LineChangeCounts::Exact {
                unchanged_lines: 1,
                inserted_lines: 1,
                removed_lines: 1,
            }
        );
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].kind(), ChangeHunkKind::Modified);
        assert_eq!(hunks[0].alignment(), HunkAlignment::ExactMinimal);
        assert_eq!(hunks[0].before_lines(), Some(1..=1));
        assert_eq!(hunks[0].after_lines(), Some(1..=1));
        assert_eq!(hunks[0].before_bytes(), 0..3);
        assert_eq!(hunks[0].after_bytes(), 0..2);
    } else {
        return Err("expected Changed explanation for line-ending-only edit".into());
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
        assert_eq!(hunks[0].alignment(), HunkAlignment::ExactMinimal);
        assert_eq!(hunks[0].before_lines(), Some(2..=2));
        assert_eq!(hunks[0].after_lines(), Some(2..=2));
        assert_eq!(hunks[1].kind(), ChangeHunkKind::Inserted);
        assert_eq!(hunks[1].alignment(), HunkAlignment::ExactMinimal);
        assert_eq!(hunks[1].before_lines(), None);
        assert_eq!(hunks[1].after_lines(), Some(5..=5));
    } else {
        return Err("expected multi-hunk Changed explanation".into());
    }

    // Verify distant edits separated by > MAX_LCS_CELLS:
    // - Unique lines use Patience LIS anchor partitioning (`LineChangeCounts::AnchorPartitioned`
    //   and `HunkAlignment::UniqueAnchorPartitioned`).
    // - Duplicate lines without unique anchors use bounded Myers (`LineChangeCounts::Exact`
    //   and `HunkAlignment::ExactMinimal`).
    for use_unique_lines in [true, false] {
        let mut distant_left = String::new();
        let mut distant_right = String::new();
        for line_no in 1..=1_200 {
            if line_no == 2 || line_no == 1_199 {
                distant_left.push_str(&format!("line-{line_no}-before\n"));
                distant_right.push_str(&format!("line-{line_no}-edited\n"));
            } else {
                let base_line = if use_unique_lines {
                    format!("line-{line_no}\n")
                } else {
                    "repeated-boilerplate-line\n".to_owned()
                };
                distant_left.push_str(&base_line);
                distant_right.push_str(&base_line);
            }
        }
        let dist_before = snapshot(Capture::Complete(&distant_left), "dist-v1", 10)?;
        let dist_after = snapshot(Capture::Complete(&distant_right), "dist-v2", 20)?;
        let dist_eval = evaluate(
            &rule,
            &dist_before,
            &dist_after,
            UNIX_EPOCH + Duration::from_secs(25),
            || Ok(()),
        )?;
        if let ChangeExplanation::Changed {
            before_line_count,
            after_line_count,
            line_counts,
            hunks,
            ..
        } = dist_eval.explain_changes(ChangeExplanationLimits::default_limits(), || Ok(()))?
        {
            assert_eq!(before_line_count, 1_200);
            assert_eq!(after_line_count, 1_200);
            let expected_alignment = if use_unique_lines {
                assert_eq!(
                    line_counts,
                    LineChangeCounts::AnchorPartitioned {
                        matched_unchanged_lines: 1_198,
                        inserted_lines: 2,
                        removed_lines: 2,
                    }
                );
                HunkAlignment::UniqueAnchorPartitioned
            } else {
                assert_eq!(
                    line_counts,
                    LineChangeCounts::Exact {
                        unchanged_lines: 1_198,
                        inserted_lines: 2,
                        removed_lines: 2,
                    }
                );
                HunkAlignment::ExactMinimal
            };
            assert_eq!(hunks.len(), 2);
            assert_eq!(hunks[0].kind(), ChangeHunkKind::Modified);
            assert_eq!(hunks[0].alignment(), expected_alignment);
            assert_eq!(hunks[0].before_lines(), Some(2..=2));
            assert_eq!(hunks[0].after_lines(), Some(2..=2));
            assert_eq!(hunks[1].kind(), ChangeHunkKind::Modified);
            assert_eq!(hunks[1].alignment(), expected_alignment);
            assert_eq!(hunks[1].before_lines(), Some(1_199..=1_199));
            assert_eq!(hunks[1].after_lines(), Some(1_199..=1_199));
            let mut reconstructed = distant_left.clone();
            let mut reverse_reconstructed = distant_right.clone();
            for hunk in hunks.iter().rev() {
                let rep = distant_right
                    .get(hunk.after_bytes())
                    .ok_or("invalid distant after_bytes")?;
                let orig = distant_left
                    .get(hunk.before_bytes())
                    .ok_or("invalid distant before_bytes")?;
                reconstructed.replace_range(hunk.before_bytes(), rep);
                reverse_reconstructed.replace_range(hunk.after_bytes(), orig);
            }
            assert_eq!(reconstructed, distant_right);
            assert_eq!(reverse_reconstructed, distant_left);
        } else {
            return Err("expected Changed explanation for distant edits".into());
        }
    }

    // Verify `[P, x×600, Q]` vs `[Q, x×600, P]`: pinning unique anchor `P` causes duplicate
    // `x` lines moved across `P` to be emitted as removed and re-inserted, and the explanation
    // explicitly reports `LineChangeCounts::AnchorPartitioned` and `HunkAlignment::UniqueAnchorPartitioned`
    // rather than claiming a globally minimal `LineChangeCounts::Exact` edit script.
    let mut swap_left = String::from("P-unique\n");
    let mut swap_right = String::from("Q-unique\n");
    for _ in 0..600 {
        swap_left.push_str("dup-x\n");
        swap_right.push_str("dup-x\n");
    }
    swap_left.push_str("Q-unique\n");
    swap_right.push_str("P-unique\n");
    let swap_before = snapshot(Capture::Complete(&swap_left), "swap-v1", 10)?;
    let swap_after = snapshot(Capture::Complete(&swap_right), "swap-v2", 20)?;
    let swap_eval = evaluate(
        &rule,
        &swap_before,
        &swap_after,
        UNIX_EPOCH + Duration::from_secs(25),
        || Ok(()),
    )?;
    if let ChangeExplanation::Changed {
        line_counts, hunks, ..
    } = swap_eval.explain_changes(ChangeExplanationLimits::default_limits(), || Ok(()))?
    {
        assert_eq!(
            line_counts,
            LineChangeCounts::AnchorPartitioned {
                matched_unchanged_lines: 1,
                inserted_lines: 601,
                removed_lines: 601,
            }
        );
        assert_eq!(hunks.len(), 2);
        assert!(
            hunks
                .iter()
                .all(|h| h.alignment() == HunkAlignment::UniqueAnchorPartitioned)
        );
        assert_eq!(hunks[0].kind(), ChangeHunkKind::Removed);
        assert_eq!(hunks[0].before_lines(), Some(1..=601));
        assert_eq!(hunks[0].after_lines(), None);
        assert_eq!(hunks[1].kind(), ChangeHunkKind::Inserted);
        assert_eq!(hunks[1].before_lines(), None);
        assert_eq!(hunks[1].after_lines(), Some(2..=602));
    } else {
        return Err("expected AnchorPartitioned Changed explanation".into());
    }

    // Verify selective sub-window coalescing: when a capture has an edit separated by
    // unique anchors from a 550-line unanchored rewrite block (> MAX_LCS_CELLS and > MAX_MYERS_EDIT_DISTANCE),
    // the early edit is marked `UniqueAnchorPartitioned` while the unanchored rewrite block is marked
    // `Coalesced(NoUniqueAnchorsOverEditBudget)` and reports `CoalescedWindowUpperBound`.
    let mut fallback_left = String::new();
    let mut fallback_right = String::new();
    fallback_left.push_str("header-1\nedit-2-before\n");
    fallback_right.push_str("header-1\nedit-2-after\n");
    for anchor_no in 3..=10 {
        let anchor = format!("anchor-{anchor_no}\n");
        fallback_left.push_str(&anchor);
        fallback_right.push_str(&anchor);
    }
    for rewrite_no in 11..=560 {
        fallback_left.push_str(&format!("old-block-{rewrite_no}\n"));
        fallback_right.push_str(&format!("new-block-{rewrite_no}\n"));
    }
    fallback_left.push_str("footer-561\n");
    fallback_right.push_str("footer-561\n");

    let fb_before = snapshot(Capture::Complete(&fallback_left), "fb-v1", 10)?;
    let fb_after = snapshot(Capture::Complete(&fallback_right), "fb-v2", 20)?;
    let fb_eval = evaluate(
        &rule,
        &fb_before,
        &fb_after,
        UNIX_EPOCH + Duration::from_secs(25),
        || Ok(()),
    )?;
    if let ChangeExplanation::Changed {
        before_line_count,
        after_line_count,
        line_counts,
        hunks,
        ..
    } = fb_eval.explain_changes(ChangeExplanationLimits::default_limits(), || Ok(()))?
    {
        assert_eq!(before_line_count, 561);
        assert_eq!(after_line_count, 561);
        assert_eq!(
            line_counts,
            LineChangeCounts::CoalescedWindowUpperBound {
                trimmed_unchanged_lines: 10,
                coalesced_before_window_lines: 551,
                coalesced_after_window_lines: 551,
            }
        );
        assert_eq!(hunks.len(), 2);
        assert_eq!(hunks[0].kind(), ChangeHunkKind::Modified);
        assert_eq!(hunks[0].alignment(), HunkAlignment::UniqueAnchorPartitioned);
        assert_eq!(hunks[0].before_lines(), Some(2..=2));
        assert_eq!(hunks[0].after_lines(), Some(2..=2));
        assert_eq!(hunks[1].kind(), ChangeHunkKind::Modified);
        assert_eq!(
            hunks[1].alignment(),
            HunkAlignment::Coalesced(CoalescedReason::NoUniqueAnchorsOverEditBudget)
        );
        assert_eq!(hunks[1].before_lines(), Some(11..=560));
        assert_eq!(hunks[1].after_lines(), Some(11..=560));
        let mut reconstructed = fallback_left.clone();
        let mut reverse_reconstructed = fallback_right.clone();
        for hunk in hunks.iter().rev() {
            let rep = fallback_right
                .get(hunk.after_bytes())
                .ok_or("invalid fallback after_bytes")?;
            let orig = fallback_left
                .get(hunk.before_bytes())
                .ok_or("invalid fallback before_bytes")?;
            reconstructed.replace_range(hunk.before_bytes(), rep);
            reverse_reconstructed.replace_range(hunk.after_bytes(), orig);
        }
        assert_eq!(reconstructed, fallback_right);
        assert_eq!(reverse_reconstructed, fallback_left);
    } else {
        return Err("expected Changed explanation for coalesced fallback".into());
    }

    // Verify strict `MAX_ALIGNMENT_STEPS` ceiling: when repeated oversized unanchored duplicate
    // blocks exhaust `MAX_ALIGNMENT_STEPS` inside `try_bounded_myers`, the remaining sub-window
    // coalesces with `HunkAlignment::Coalesced(CoalescedReason::AlignmentStepBudgetExceeded)`.
    let mut step_left = String::new();
    let mut step_right = String::new();
    for block in 0..16 {
        for idx in 0..520 {
            if !(130..518).contains(&idx) {
                step_left.push_str("dup-only-left\n");
                step_right.push_str("dup-only-right\n");
            } else {
                step_left.push_str("dup-shared\n");
                step_right.push_str("dup-shared\n");
            }
        }
        let sep = format!("unique-sep-{block}\n");
        step_left.push_str(&sep);
        step_right.push_str(&sep);
    }
    let step_before = snapshot(Capture::Complete(&step_left), "step-v1", 10)?;
    let step_after = snapshot(Capture::Complete(&step_right), "step-v2", 20)?;
    let step_eval = evaluate(
        &rule,
        &step_before,
        &step_after,
        UNIX_EPOCH + Duration::from_secs(25),
        || Ok(()),
    )?;
    if let ChangeExplanation::Changed {
        line_counts, hunks, ..
    } = step_eval.explain_changes(ChangeExplanationLimits::new(16_384, 64)?, || Ok(()))?
    {
        assert!(matches!(
            line_counts,
            LineChangeCounts::CoalescedWindowUpperBound { .. }
        ));
        assert!(hunks.iter().any(|h| {
            h.alignment() == HunkAlignment::Coalesced(CoalescedReason::AlignmentStepBudgetExceeded)
        }));
    } else {
        return Err("expected Changed explanation for step-budget coalescing".into());
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
    let line_limited = ChangeExplanationLimits::new(3, 10)?;
    assert_eq!(
        multi_eval.explain_changes(line_limited, || Ok(())),
        Err(ChangeError::LineLimitExceeded {
            actual_lines: 5,
            max_lines: 3,
        })
    );
    // Notification projection preserves the rule decision and replacement span while marking
    // the line-level explanation Unavailable when a line or hunk limit is hit.
    let line_limited_notif = multi_eval.explain_notification(line_limited, || Ok(()))?;
    assert_eq!(line_limited_notif.decision(), RuleDecision::ChangeDetected);
    assert_eq!(
        line_limited_notif.replacement_span(),
        multi_eval.comparison()
    );
    assert_eq!(
        line_limited_notif.explanation(),
        &ChangeExplanation::Unavailable {
            before_line_count: 5,
            after_line_count: 5,
            reason: ExplanationUnavailableReason::LineLimitExceeded {
                actual_lines: 5,
                max_lines: 3,
            },
        }
    );
    // Unchanged captures over max_lines_per_capture still succeed without allocating a line table.
    let same_after = snapshot(
        Capture::Complete("L1\nChangeA\nKeep\nChangeB\nL5\n"),
        "same-hunk",
        20,
    )?;
    let same_eval = evaluate(&rule, &multi_before, &same_after, at, || Ok(()))?;
    assert_eq!(
        same_eval.explain_changes(line_limited, || Ok(())),
        Ok(ChangeExplanation::Unchanged {
            method_id: EXPLANATION_METHOD_ID,
            line_count: 5,
        })
    );
    let hunk_limited = ChangeExplanationLimits::new(10, 1)?;
    assert_eq!(
        multi_eval.explain_changes(hunk_limited, || Ok(())),
        Err(ChangeError::HunkLimitExceeded { max_hunks: 1 })
    );
    let hunk_limited_notif = multi_eval.explain_notification(hunk_limited, || Ok(()))?;
    assert_eq!(hunk_limited_notif.decision(), RuleDecision::ChangeDetected);
    assert_eq!(
        hunk_limited_notif.replacement_span(),
        multi_eval.comparison()
    );
    assert_eq!(
        hunk_limited_notif.explanation(),
        &ChangeExplanation::Unavailable {
            before_line_count: 5,
            after_line_count: 5,
            reason: ExplanationUnavailableReason::HunkLimitExceeded { max_hunks: 1 },
        }
    );
    assert_eq!(
        multi_eval.explain_changes(ChangeExplanationLimits::default_limits(), || {
            Err(StopReason::Superseded)
        }),
        Err(ChangeError::Stopped(StopReason::Superseded))
    );
    assert_eq!(
        multi_eval.explain_notification(line_limited, || Err(StopReason::Superseded)),
        Err(ChangeError::Stopped(StopReason::Superseded))
    );
    Ok(())
}
