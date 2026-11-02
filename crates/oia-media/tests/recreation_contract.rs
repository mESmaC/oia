//! Targeted contract tests for illustrative recreation-prompt export and imported-recreation
//! labeling (`MEDIA-04`, `A10`, `AGENTS.md` §§2, 5, 8).

use oia_domain::{OpaqueId, VersionId};
use oia_media::recreation::{
    AccountVariant, ApparentScale, AssetExportPolicy, AttemptedArtifactClass, ContextField,
    ExplicitAssumption, ExplicitUnknown, ImportedRecreationInput, RecreationError,
    RecreationLimitKind, RecreationLimits, RecreationPromptInput, ReferenceAssetInput,
    ReportedDetail, SceneContext, VISIBLE_RECREATION_LABEL, export_recreation_prompt,
    validate_imported_recreation,
};

fn sample_prompt_input() -> Result<RecreationPromptInput, RecreationError> {
    Ok(RecreationPromptInput {
        prompt_id: OpaqueId::new("prompt:a10-recreation")?,
        prompt_version: VersionId::new("ver:prompt-a10-v1")?,
        case_id: OpaqueId::new("case:a10")?,
        title: "Conflicting witness account recreation".to_owned(),
        context: SceneContext {
            scene: ContextField::Reported {
                description: "Coastal bluff looking west over bay".to_owned(),
                source_versions: vec![VersionId::new("ver:witness-1")?],
            },
            time_context: ContextField::Reported {
                description: "2026-10-14 02:10 UTC".to_owned(),
                source_versions: vec![VersionId::new("ver:witness-1")?],
            },
            viewpoint: ContextField::Assumed {
                description: "Eye-level 35mm-equivalent framing toward azimuth 270 deg".to_owned(),
                assumption_id: OpaqueId::new("assumption:camera-fov")?,
            },
            lighting: ContextField::Unknown {
                reason: "Cloud base and moon illumination fraction were not recorded".to_owned(),
            },
            apparent_scale: ApparentScale::ReportedAngularOrRelative {
                description: "Roughly full-moon diameter (~0.5 deg) at unknown distance".to_owned(),
                source_versions: vec![
                    VersionId::new("ver:witness-1")?,
                    VersionId::new("ver:witness-2")?,
                ],
                physical_size_unknown_reason:
                    "Single-azimuth observation without rangefinder or second baseline".to_owned(),
            },
        },
        shared_reported_details: vec![ReportedDetail {
            detail_id: OpaqueId::new("detail:horizontal-path")?,
            statement: "Moved horizontally from north to south across the bay for 14 seconds"
                .to_owned(),
            source_versions: vec![
                VersionId::new("ver:witness-1")?,
                VersionId::new("ver:witness-2")?,
            ],
            source_note: Some("Both witnesses timed duration against watch".to_owned()),
        }],
        conflicting_variants: vec![
            AccountVariant {
                variant_id: OpaqueId::new("variant:single-orb")?,
                summary: "Single orange luminous sphere with sharp rim".to_owned(),
                reported_details: vec![ReportedDetail {
                    detail_id: OpaqueId::new("detail:single-rim")?,
                    statement: "Single continuous orange sphere without separated lights"
                        .to_owned(),
                    source_versions: vec![VersionId::new("ver:witness-1")?],
                    source_note: None,
                }],
                conflicts_with: vec![OpaqueId::new("variant:tandem-pair")?],
            },
            AccountVariant {
                variant_id: OpaqueId::new("variant:tandem-pair")?,
                summary: "Two white-amber lights separated by ~1 deg horizontally".to_owned(),
                reported_details: vec![ReportedDetail {
                    detail_id: OpaqueId::new("detail:two-lights")?,
                    statement: "Two distinct lights maintaining fixed horizontal separation"
                        .to_owned(),
                    source_versions: vec![VersionId::new("ver:witness-2")?],
                    source_note: Some("Observed through 8x42 binoculars".to_owned()),
                }],
                conflicts_with: vec![OpaqueId::new("variant:single-orb")?],
            },
        ],
        assumptions: vec![ExplicitAssumption {
            assumption_id: OpaqueId::new("assumption:camera-fov")?,
            assumption: "35mm-equivalent framing chosen for horizon context".to_owned(),
            rationale: "Visual testimony has no focal length; framing is illustrative only"
                .to_owned(),
        }],
        unknowns: vec![ExplicitUnknown {
            unknown_id: OpaqueId::new("unknown:object-altitude")?,
            aspect: "True altitude above sea level and slant range".to_owned(),
            impact_on_recreation:
                "Rendered height above water is illustrative and not a measurement".to_owned(),
        }],
        reference_assets: vec![
            ReferenceAssetInput {
                asset_id: OpaqueId::new("asset:coastline-dem")?,
                source_version: VersionId::new("ver:coastline-dem-v1")?,
                sha256_hex: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_owned(),
                media_type: "image/png".to_owned(),
                role: "Coastline horizon contour".to_owned(),
                redistribution_permitted: true,
                usage_conditions: "CC-BY-4.0; retain attribution".to_owned(),
            },
            ReferenceAssetInput {
                asset_id: OpaqueId::new("asset:restricted-photo")?,
                source_version: VersionId::new("ver:restricted-photo-v1")?,
                sha256_hex: "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210"
                    .to_owned(),
                media_type: "image/jpeg".to_owned(),
                role: "Private dock angle reference".to_owned(),
                redistribution_permitted: false,
                usage_conditions: "Internal case review only; no redistribution".to_owned(),
            },
        ],
    })
}

/// Risk addressed (`MEDIA-04`, `A10`): conflicting witness accounts, assumptions, unknowns,
/// uncertain scale, and restricted assets must remain explicitly separated in exported prompts,
/// and imported recreations must retain prompt/settings lineage and visible labels while refusing
/// promotion to observation or measurement evidence.
#[test]
fn conflicting_accounts_and_imported_recreation_preserve_separation_and_labels()
-> Result<(), RecreationError> {
    let limits = RecreationLimits::new(16, 8, 8, 4, 8, 8192, 16_384)?;
    let input = sample_prompt_input()?;

    // Refusal mode rejects unpermitted assets rather than exporting them.
    assert_eq!(
        Err(RecreationError::UnpermittedAsset {
            asset_id: OpaqueId::new("asset:restricted-photo")?,
        }),
        export_recreation_prompt(&input, limits, AssetExportPolicy::RefuseUnpermitted)
    );

    // Omit-with-notice mode exports permitted assets and records an explicit omission notice.
    let exported = export_recreation_prompt(&input, limits, AssetExportPolicy::OmitWithNotice)?;
    assert_eq!(exported.permitted_assets.len(), 1);
    assert_eq!(
        exported
            .permitted_assets
            .first()
            .map(|a| a.asset_id.as_str()),
        Some("asset:coastline-dem")
    );
    assert_eq!(exported.omitted_assets.len(), 1);
    assert_eq!(
        exported.omitted_assets.first().map(|a| a.asset_id.as_str()),
        Some("asset:restricted-photo")
    );

    let text = &exported.portable_prompt_text;
    assert!(text.contains(VISIBLE_RECREATION_LABEL));
    assert!(
        text.contains("### Variant `variant:single-orb` (conflicts_with: [variant:tandem-pair])")
    );
    assert!(
        text.contains("### Variant `variant:tandem-pair` (conflicts_with: [variant:single-orb])")
    );
    assert!(text.contains("- viewpoint: [ASSUMED | assumption: assumption:camera-fov]"));
    assert!(text.contains("- lighting: [UNKNOWN]"));
    assert!(text.contains(
        "- apparent_scale: [REPORTED ANGULAR/RELATIVE | sources: ver:witness-1, ver:witness-2]"
    ));
    assert!(text.contains("## Omitted Reference Assets (Redistribution Restricted)"));

    // Valid imported recreation preserves lineage, settings, and non-evidentiary flags.
    let import_input = ImportedRecreationInput {
        artifact_id: OpaqueId::new("artifact:rec-1")?,
        artifact_version: VersionId::new("ver:rec-1-v1")?,
        sha256_hex: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
        media_type: "image/png".to_owned(),
        prompt_id: exported.prompt_id.clone(),
        prompt_version: exported.prompt_version.clone(),
        variant_id: Some(OpaqueId::new("variant:tandem-pair")?),
        generator_model: "flux-illustrative-v1".to_owned(),
        generator_settings: "seed=108, steps=28".to_owned(),
        attempted_class: AttemptedArtifactClass::IllustrativeRecreation,
        visible_label: VISIBLE_RECREATION_LABEL.to_owned(),
    };
    let validated = validate_imported_recreation(&import_input, &exported, 2048)?;
    assert!(!validated.can_serve_as_observation_evidence);
    assert!(!validated.can_establish_physical_measurement);

    // Promoting a generated recreation to observation evidence or measurement is refused.
    for forbidden in [
        AttemptedArtifactClass::ObservationEvidence,
        AttemptedArtifactClass::SourceAssertion,
        AttemptedArtifactClass::CalibratedMeasurement,
    ] {
        let mut promoted = import_input.clone();
        promoted.attempted_class = forbidden;
        assert_eq!(
            Err(RecreationError::ForbiddenEvidencePromotion {
                artifact_id: OpaqueId::new("artifact:rec-1")?,
                attempted: forbidden,
            }),
            validate_imported_recreation(&promoted, &exported, 2048)
        );
    }

    // Stripping the visible recreation label is refused.
    let mut unlabeled = import_input;
    unlabeled.visible_label = "Photographic evidence capture".to_owned();
    assert_eq!(
        Err(RecreationError::MissingVisibleRecreationLabel {
            artifact_id: OpaqueId::new("artifact:rec-1")?,
        }),
        validate_imported_recreation(&unlabeled, &exported, 2048)
    );

    Ok(())
}

/// Risk addressed (`MEDIA-04`, `AGENTS.md` §§2, 5): unattributed reported details, undeclared
/// assumptions, broken conflict references, invalid digests, and oversized inputs must be
/// rejected before producing a prompt rather than silently truncated or defaulted.
#[test]
fn rejects_unattributed_details_undeclared_assumptions_and_oversized_inputs()
-> Result<(), RecreationError> {
    let limits = RecreationLimits::new(16, 8, 8, 4, 8, 8192, 16_384)?;

    // Unattributed reported detail is rejected.
    let mut unattributed = sample_prompt_input()?;
    if let Some(first) = unattributed.shared_reported_details.first_mut() {
        first.source_versions.clear();
    }
    assert_eq!(
        Err(RecreationError::MissingSourceVersions {
            item_id: OpaqueId::new("detail:horizontal-path")?,
        }),
        export_recreation_prompt(&unattributed, limits, AssetExportPolicy::OmitWithNotice)
    );

    // Scene field referencing an undeclared assumption is rejected.
    let mut undeclared_assumption = sample_prompt_input()?;
    undeclared_assumption.context.viewpoint = ContextField::Assumed {
        description: "Assumed telephoto lens".to_owned(),
        assumption_id: OpaqueId::new("assumption:missing")?,
    };
    assert_eq!(
        Err(RecreationError::UnknownAssumptionReference {
            field: "context.viewpoint",
            assumption_id: OpaqueId::new("assumption:missing")?,
        }),
        export_recreation_prompt(
            &undeclared_assumption,
            limits,
            AssetExportPolicy::OmitWithNotice
        )
    );

    // Output budget smaller than rendered text is rejected without truncation.
    let tiny_output_limits = RecreationLimits::new(16, 8, 8, 4, 8, 8192, 128)?;
    let valid_input = sample_prompt_input()?;
    assert!(matches!(
        export_recreation_prompt(
            &valid_input,
            tiny_output_limits,
            AssetExportPolicy::OmitWithNotice
        ),
        Err(RecreationError::LimitExceeded {
            kind: RecreationLimitKind::OutputBytes,
            ..
        })
    ));

    Ok(())
}
