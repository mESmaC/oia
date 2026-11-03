#[path = "../examples/support/recreation_fixture.rs"]
mod recreation_fixture;

use oia_analysis::statement_comparison::{CandidateDismissal, CodedValue, StopReason};
use oia_app::recreation_composer::{
    ChangedSection, CompositionError, MAX_OPTIONS, compose_recreation_prompt,
};
use oia_domain::{ActorId, OpaqueId, VersionId};
use oia_media::recreation::{
    ApparentScale, AssetExportPolicy, ContextField, RecreationError, VISIBLE_RECREATION_LABEL,
};
use recreation_fixture::{Fixture, assertion, limits};
use std::error::Error;

#[test]
fn composition_preserves_evidence_and_regeneration() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new()?;
    let mut sources = fixture.sources()?;
    sources.get_mut(2).ok_or("missing witness")?.exposure =
        oia_analysis::statement_comparison::ExplanationExposure::AfterExplanations;
    let first = compose_recreation_prompt(
        fixture.request(&sources, "prompt-version:1")?,
        None,
        limits()?,
        AssetExportPolicy::OmitWithNotice,
        || Ok(()),
    )?;
    let original = first.clone();
    assert!(
        first
            .portable_text()
            .starts_with(first.exported().portable_prompt_text())
    );
    assert!(first.portable_text().contains(VISIBLE_RECREATION_LABEL));
    assert!(first.portable_text().contains("generated, unreviewed"));
    assert!(
        first
            .portable_text()
            .contains("Partial comparison: true; contains simulated input: true")
    );
    assert!(
        first
            .portable_text()
            .contains("Only one paragraph captured")
    );
    assert_eq!(first.comparison().declared.reported_group_count, 2);
    assert_eq!(
        first.comparison().candidate_adjusted.reported_group_count,
        2
    );
    assert!(!first.comparison().copy_candidates.is_empty());
    assert_eq!(first.input().conflicting_variants.len(), 2);
    let all_details: Vec<_> = first
        .input()
        .shared_reported_details
        .iter()
        .chain(
            first
                .input()
                .conflicting_variants
                .iter()
                .flat_map(|variant| variant.reported_details.iter()),
        )
        .collect();
    let simulation_version = &sources.get(3).ok_or("missing simulation")?.version;
    let witness_version = &sources.get(2).ok_or("missing witness")?.version;
    assert!(
        all_details
            .iter()
            .filter(|detail| detail.source_versions.contains(witness_version))
            .all(|detail| detail.source_note.as_deref().is_some_and(
                |note| note.contains("recorded after candidate explanations were exposed")
            ))
    );
    assert!(
        all_details
            .iter()
            .all(|detail| { !detail.source_versions.contains(simulation_version) })
    );
    let orange = first
        .input()
        .conflicting_variants
        .iter()
        .find(|variant| variant.summary.contains("orange"))
        .ok_or("missing orange alternative")?;
    let orange_detail = orange
        .reported_details
        .first()
        .ok_or("missing orange report")?;
    assert_eq!(orange_detail.source_versions.len(), 2);
    assert!(orange_detail.statement.contains("PoliceReport"));
    assert!(orange_detail.statement.contains("NewsReport"));
    assert!(orange_detail.statement.contains("orange light"));
    assert!(orange_detail.source_note.as_deref().is_some_and(|note| {
        note.contains("declared reported groups=1; candidate-adjusted reported groups=1")
    }));
    assert!(matches!(
        first.input().context.scene,
        ContextField::Reported { .. }
    ));
    assert!(matches!(
        first.input().context.time_context,
        ContextField::Assumed { .. }
    ));
    assert!(matches!(
        first.input().context.lighting,
        ContextField::Unknown { .. }
    ));
    assert!(matches!(
        first.input().context.apparent_scale,
        ApparentScale::Unknown { .. }
    ));
    for aspect in ["size", "geometry", "distance", "sound"] {
        assert!(
            first
                .input()
                .unknowns
                .iter()
                .any(|unknown| unknown.aspect == aspect)
        );
    }
    assert_eq!(first.exported().permitted_assets().len(), 1);
    assert_eq!(first.exported().omitted_assets().len(), 1);
    assert!(first.input().negative_keywords.is_empty());

    const UNICODE: &str = "मैंने एक नारंगी रोशनी देखी e\u{301} \"quoted\" \\ trail";
    const ASPECT: &str = "रंग e\u{301} \"aspect\" \\";
    const CODE: &str = "नारंगी e\u{301} \"code\" \\";
    let unicode_coding = [
        assertion(UNICODE, ASPECT, CodedValue::Reported(CODE), UNICODE)?,
        assertion(UNICODE, "unknown", CodedValue::StatedUnknown, UNICODE)?,
        assertion(
            UNICODE,
            "single-reported",
            CodedValue::Reported("reported"),
            UNICODE,
        )?,
    ];
    let other_coding = [assertion(
        UNICODE,
        ASPECT,
        CodedValue::Reported("other"),
        UNICODE,
    )?];
    let simulated_coding = [assertion(
        UNICODE,
        "single-reported",
        CodedValue::Reported("simulated"),
        UNICODE,
    )?];
    let mut unicode_source = sources.first().ok_or("missing police")?.clone();
    unicode_source.text = oia_analysis::statement_comparison::TextCapture::Complete(UNICODE);
    unicode_source.assertions = &unicode_coding;
    let mut other_source = sources.get(2).ok_or("missing witness")?.clone();
    other_source.text = oia_analysis::statement_comparison::TextCapture::Complete(UNICODE);
    other_source.assertions = &other_coding;
    let mut simulated_source = sources.get(3).ok_or("missing simulation")?.clone();
    simulated_source.text = oia_analysis::statement_comparison::TextCapture::Complete(UNICODE);
    simulated_source.assertions = &simulated_coding;
    let unicode_sources = [unicode_source, other_source, simulated_source];
    let unicode_prompt = compose_recreation_prompt(
        fixture.request(&unicode_sources, "prompt-version:unicode")?,
        None,
        limits()?,
        AssetExportPolicy::OmitWithNotice,
        || Ok(()),
    )?;
    let unicode_variant = unicode_prompt
        .input()
        .conflicting_variants
        .iter()
        .find(|variant| variant.summary.contains(CODE))
        .ok_or("missing Unicode variant")?;
    let unicode_detail = unicode_variant
        .reported_details
        .first()
        .ok_or("missing Unicode detail")?;
    assert!(unicode_detail.statement.contains(UNICODE));
    assert!(unicode_detail.statement.contains(ASPECT));
    assert!(unicode_detail.statement.contains(CODE));
    assert!(
        unicode_prompt
            .comparison()
            .aspects
            .iter()
            .flat_map(|aspect| &aspect.reports)
            .all(|report| report.wording_text == UNICODE)
    );
    assert!(
        unicode_prompt
            .input()
            .unknowns
            .iter()
            .filter(|unknown| matches!(unknown.aspect.as_str(), "unknown" | "single-reported"))
            .all(|unknown| unknown.impact_on_recreation.contains(UNICODE))
    );
    assert!(
        unicode_prompt
            .exported()
            .portable_prompt_text()
            .contains("मैंने एक नारंगी रोशनी देखी e\u{301} \\\"quoted\\\" \\\\ trail")
    );
    assert!(
        unicode_prompt
            .exported()
            .portable_prompt_text()
            .contains("रंग e\u{301} \\\"aspect\\\" \\\\")
    );
    assert!(
        !unicode_prompt
            .exported()
            .portable_prompt_text()
            .contains("\\u{301}")
    );
    assert!(
        unicode_prompt
            .input()
            .shared_reported_details
            .iter()
            .any(|detail| detail
                .source_note
                .as_deref()
                .is_some_and(|note| note
                    .contains("conflict only with excluded non-reported/simulated material")))
    );

    const GREEN: &str = "The later account described a green light.";
    let revised_coding = [assertion(
        GREEN,
        "colour",
        CodedValue::Reported("green"),
        "green light",
    )?];
    let mut revised_source = sources.get(2).ok_or("missing witness")?.clone();
    revised_source.revision = 1;
    revised_source.version = VersionId::new("version:witness-2")?;
    revised_source.text = oia_analysis::statement_comparison::TextCapture::Complete(GREEN);
    revised_source.assertions = &revised_coding;
    let mut revised_sources = sources.clone();
    revised_sources.push(revised_source);
    let with_revision = compose_recreation_prompt(
        fixture.request(&revised_sources, "prompt-version:revision")?,
        Some(&first),
        limits()?,
        AssetExportPolicy::OmitWithNotice,
        || Ok(()),
    )?;
    assert!(
        with_revision
            .portable_text()
            .contains("Non-representative revision version:witness-2")
    );
    assert!(with_revision.portable_text().contains(
        "analyst code \"green\"; source version:witness-2 bytes 30..41: \"green light\""
    ));
    assert!(
        !with_revision
            .exported()
            .portable_prompt_text()
            .contains("green light")
    );
    assert_eq!(first, original);

    let mut undeclared = sources.clone();
    undeclared.get_mut(1).ok_or("missing news")?.derived_from = &[];
    let candidate_only = compose_recreation_prompt(
        fixture.request(&undeclared, "prompt-version:copy-1")?,
        None,
        limits()?,
        AssetExportPolicy::OmitWithNotice,
        || Ok(()),
    )?;
    assert_eq!(candidate_only.comparison().declared.reported_group_count, 3);
    assert_eq!(
        candidate_only
            .comparison()
            .candidate_adjusted
            .reported_group_count,
        2
    );
    let candidate = candidate_only
        .comparison()
        .copy_candidates
        .first()
        .ok_or("missing copy candidate")?;
    let reviews = [CandidateDismissal {
        a_version: undeclared
            .get(candidate.a)
            .ok_or("missing candidate source")?
            .version
            .clone(),
        a_range: candidate.a_range.clone(),
        b_version: undeclared
            .get(candidate.b)
            .ok_or("missing candidate source")?
            .version
            .clone(),
        b_range: candidate.b_range.clone(),
        reviewed_by: ActorId::new("actor:reviewer")?,
        reason: "Synthetic dismissal to verify that candidate dependence is reversible",
    }];
    let mut review_request = fixture.request(&undeclared, "prompt-version:copy-2")?;
    review_request.comparison.dismissals = &reviews;
    let reviewed = compose_recreation_prompt(
        review_request,
        Some(&candidate_only),
        limits()?,
        AssetExportPolicy::OmitWithNotice,
        || Ok(()),
    )?;
    assert_eq!(
        reviewed
            .comparison()
            .candidate_adjusted
            .reported_group_count,
        3
    );
    assert!(
        reviewed
            .changes()
            .sections
            .contains(&ChangedSection::CandidateReview)
    );
    assert!(reviewed.portable_text().contains("actor:reviewer"));

    let mut expanded = sources.clone();
    let mut late = sources.get(2).ok_or("missing witness")?.clone();
    late.account = OpaqueId::new("account:late")?;
    late.version = VersionId::new("version:late-1")?;
    expanded.push(late);
    let second = compose_recreation_prompt(
        fixture.request(&expanded, "prompt-version:2")?,
        Some(&first),
        limits()?,
        AssetExportPolicy::OmitWithNotice,
        || Ok(()),
    )?;
    assert_eq!(first, original);
    assert_eq!(
        second.changes().predecessor.as_ref(),
        Some(&VersionId::new("prompt-version:1")?)
    );
    assert_eq!(
        second.changes().added_sources,
        vec![VersionId::new("version:late-1")?]
    );
    assert!(!second.changes().changed_details.is_empty());
    assert_eq!(second.changes().added_details.len(), 0);
    assert!(!second.changes().changed_accounts.is_empty());

    let without_news: Vec<_> = expanded
        .iter()
        .filter(|source| {
            source.class != oia_analysis::statement_comparison::SourceClass::NewsReport
        })
        .cloned()
        .collect();
    let third = compose_recreation_prompt(
        fixture.request(&without_news, "prompt-version:3")?,
        Some(&second),
        limits()?,
        AssetExportPolicy::OmitWithNotice,
        || Ok(()),
    )?;
    assert_eq!(
        third.changes().removed_sources,
        vec![VersionId::new("version:news-1")?]
    );
    assert_eq!(
        Err(CompositionError::ReusedPromptVersion),
        compose_recreation_prompt(
            fixture.request(&without_news, "prompt-version:1")?,
            Some(&third),
            limits()?,
            AssetExportPolicy::OmitWithNotice,
            || Ok(())
        )
    );
    Ok(())
}

#[test]
fn composition_refuses_identity_permission_and_control_failures() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new()?;
    let sources = fixture.sources()?;
    for wording in ["cited\ttext", "cited\u{202e}text"] {
        let coding = [assertion(
            wording,
            "colour",
            CodedValue::Reported("orange"),
            wording,
        )?];
        let mut source = sources.first().ok_or("missing police")?.clone();
        source.text = oia_analysis::statement_comparison::TextCapture::Complete(wording);
        source.assertions = &coding;
        let expected = CompositionError::SourceTextControl {
            source: source.version.clone(),
            assertion: Some(0),
            field: "wording",
        };
        assert_eq!(
            Err(expected),
            compose_recreation_prompt(
                fixture.request(
                    &[source, sources.get(2).ok_or("missing witness")?.clone()],
                    "prompt-version:control"
                )?,
                None,
                limits()?,
                AssetExportPolicy::OmitWithNotice,
                || Ok(()),
            )
        );
    }
    let request = fixture.request(&sources, "prompt-version:1")?;
    assert!(matches!(
        compose_recreation_prompt(
            request.clone(),
            None,
            limits()?,
            AssetExportPolicy::RefuseUnpermitted,
            || Ok(())
        ),
        Err(CompositionError::Recreation(
            RecreationError::UnpermittedAsset { .. }
        ))
    ));
    assert_eq!(
        Err(CompositionError::Stopped(StopReason::Cancelled)),
        compose_recreation_prompt(
            request.clone(),
            None,
            limits()?,
            AssetExportPolicy::OmitWithNotice,
            || Err(StopReason::Cancelled)
        )
    );
    let first = compose_recreation_prompt(
        request.clone(),
        None,
        limits()?,
        AssetExportPolicy::OmitWithNotice,
        || Ok(()),
    )?;
    assert_eq!(
        Err(CompositionError::ReusedPromptVersion),
        compose_recreation_prompt(
            request,
            Some(&first),
            limits()?,
            AssetExportPolicy::OmitWithNotice,
            || Ok(())
        )
    );
    let mut changed = sources.clone();
    changed.first_mut().ok_or("missing police")?.attribution = "Changed source metadata";
    assert!(matches!(
        compose_recreation_prompt(
            fixture.request(&changed, "prompt-version:2")?,
            Some(&first),
            limits()?,
            AssetExportPolicy::OmitWithNotice,
            || Ok(())
        ),
        Err(CompositionError::ChangedPinnedSource(_))
    ));

    let recoded = [assertion(
        "A silent orange light hovered over the dark reservoir near the north shore for several minutes.",
        "colour",
        CodedValue::Reported("amber"),
        "orange light",
    )?];
    let mut coded_sources = sources.clone();
    coded_sources
        .first_mut()
        .ok_or("missing police")?
        .assertions = &recoded;
    assert_eq!(
        Err(CompositionError::ChangedCodingVersion),
        compose_recreation_prompt(
            fixture.request(&coded_sources, "prompt-version:2")?,
            Some(&first),
            limits()?,
            AssetExportPolicy::OmitWithNotice,
            || Ok(())
        )
    );
    let mut recoding = fixture.request(&coded_sources, "prompt-version:2")?;
    recoding.comparison.coding_version = VersionId::new("coding:fixture-2")?;
    let recoded_prompt = compose_recreation_prompt(
        recoding,
        Some(&first),
        limits()?,
        AssetExportPolicy::OmitWithNotice,
        || Ok(()),
    )?;
    assert!(
        recoded_prompt
            .changes()
            .sections
            .contains(&ChangedSection::ComparisonMethod)
    );
    assert!(!recoded_prompt.changes().added_details.is_empty());

    let assets = vec![fixture.assets.first().ok_or("missing asset")?.clone(); MAX_OPTIONS + 1];
    let mut oversized = fixture.request(&sources, "prompt-version:2")?;
    oversized.reference_assets = &assets;
    assert_eq!(
        Err(CompositionError::Limit("options")),
        compose_recreation_prompt(
            oversized,
            Some(&first),
            limits()?,
            AssetExportPolicy::OmitWithNotice,
            || Ok(())
        )
    );
    let mut checkpoints = 0;
    compose_recreation_prompt(
        fixture.request(&sources, "prompt-version:count")?,
        None,
        limits()?,
        AssetExportPolicy::OmitWithNotice,
        || {
            checkpoints += 1;
            Ok(())
        },
    )?;
    let mut current = 0;
    assert_eq!(
        Err(CompositionError::Stopped(StopReason::Superseded)),
        compose_recreation_prompt(
            fixture.request(&sources, "prompt-version:count")?,
            None,
            limits()?,
            AssetExportPolicy::OmitWithNotice,
            || {
                current += 1;
                if current == checkpoints {
                    Err(StopReason::Superseded)
                } else {
                    Ok(())
                }
            }
        )
    );
    Ok(())
}
