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
        title: "Synthetic conflicting witness account recreation".to_owned(),
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
/// and imported recreations must require an exact `VISIBLE_RECREATION_LABEL`, require variant
/// selection when variants exist, and refuse promotion to observation or measurement evidence.
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
    assert_eq!(exported.permitted_assets().len(), 1);
    assert_eq!(
        exported
            .permitted_assets()
            .first()
            .map(|a| a.asset_id.as_str()),
        Some("asset:coastline-dem")
    );
    assert_eq!(exported.omitted_assets().len(), 1);
    assert_eq!(
        exported
            .omitted_assets()
            .first()
            .map(|a| a.asset_id.as_str()),
        Some("asset:restricted-photo")
    );

    let text = exported.portable_prompt_text();
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
        prompt_id: exported.prompt_id().clone(),
        prompt_version: exported.prompt_version().clone(),
        variant_id: Some(OpaqueId::new("variant:tandem-pair")?),
        generator_model: "flux-illustrative-v1".to_owned(),
        generator_settings: "seed=108, steps=28".to_owned(),
        attempted_class: AttemptedArtifactClass::IllustrativeRecreation,
        visible_label: VISIBLE_RECREATION_LABEL.to_owned(),
    };
    let validated = validate_imported_recreation(&import_input, &exported, 2048)?;
    assert_eq!(validated.visible_label(), VISIBLE_RECREATION_LABEL);
    assert!(!validated.can_serve_as_observation_evidence());
    assert!(!validated.can_establish_physical_measurement());

    // MEDIA-04/A10: exported metadata must retain labels and lineage, and never
    // claim that a declared digest or a sidecar proves image-byte/pixel integrity.
    let mut quoted_import = import_input.clone();
    quoted_import.generator_settings =
        "seed=108, note=\"variant_id=forged\" \\ می\u{200C}خواهم \u{200F}".to_owned();
    let quoted = validate_imported_recreation(&quoted_import, &exported, 4096)?;
    let sidecar = quoted.portable_metadata_text(4096)?;
    assert!(sidecar.starts_with(VISIBLE_RECREATION_LABEL));
    for line in [
        "artifact_id=\"artifact:rec-1\"",
        "artifact_version=\"ver:rec-1-v1\"",
        "prompt_id=\"prompt:a10-recreation\"",
        "prompt_version=\"ver:prompt-a10-v1\"",
        "variant_id=\"variant:tandem-pair\"",
        "generator_model=\"flux-illustrative-v1\"",
        "media_type=\"image/png\"",
        "Observation evidence: false",
        "Physical measurement: false",
        "Digest is caller-declared; image bytes have not been verified.",
    ] {
        assert!(sidecar.lines().any(|actual| actual == line), "{line}");
    }
    assert!(sidecar.contains(&format!(
        "declared_sha256_hex=\"{}\"",
        import_input.sha256_hex
    )));
    assert!(sidecar.contains("note=\\\"variant_id=forged\\\" \\\\ می\u{200C}خواهم \\u{200f}"));
    assert!(!sidecar.contains('\u{200F}'));
    assert!(sidecar.contains(quoted.fidelity_caveat()));
    assert_eq!(quoted.portable_metadata_text(sidecar.len())?, sidecar);
    assert!(matches!(
        quoted.portable_metadata_text(sidecar.len() - 1),
        Err(RecreationError::LimitExceeded {
            kind: RecreationLimitKind::OutputBytes,
            ..
        })
    ));
    assert_eq!(
        quoted.portable_metadata_text(0),
        Err(RecreationError::ZeroLimit(RecreationLimitKind::OutputBytes))
    );

    let mut shared_input = input.clone();
    shared_input.conflicting_variants.clear();
    let shared =
        export_recreation_prompt(&shared_input, limits, AssetExportPolicy::OmitWithNotice)?;
    let mut shared_import = import_input.clone();
    shared_import.variant_id = None;
    let shared_validated = validate_imported_recreation(&shared_import, &shared, 4096)?;
    assert!(
        shared_validated
            .portable_metadata_text(4096)?
            .contains("\nvariant_id=\"\"\n")
    );

    // Omitting variant_id when the prompt has conflicting variants is rejected.
    let mut missing_variant = import_input.clone();
    missing_variant.variant_id = None;
    assert_eq!(
        Err(RecreationError::MissingVariantSelection {
            prompt_id: exported.prompt_id().clone(),
        }),
        validate_imported_recreation(&missing_variant, &exported, 2048)
    );

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

    // Substring or negated labels are refused; only exact VISIBLE_RECREATION_LABEL passes (B1).
    for bad_label in [
        "Photographic evidence capture",
        "NOT AN ILLUSTRATIVE RECREATION - VERIFIED OBSERVATION EVIDENCE",
        "xILLUSTRATIVE RECREATIONx",
    ] {
        let mut unlabeled = import_input.clone();
        unlabeled.visible_label = bad_label.to_owned();
        assert_eq!(
            Err(RecreationError::MissingVisibleRecreationLabel {
                artifact_id: OpaqueId::new("artifact:rec-1")?,
            }),
            validate_imported_recreation(&unlabeled, &exported, 2048)
        );
    }

    Ok(())
}

/// Risk addressed (`MEDIA-04`, PR #83 review): legitimate multilingual text and emoji must not be
/// rejected, while invisible formatting/tag characters must remain visible to prompt reviewers and
/// downstream model users rather than disappearing into exported text.
#[test]
fn preserves_joiners_and_visibly_escapes_invisible_formatting() -> Result<(), RecreationError> {
    let limits = RecreationLimits::new(16, 8, 8, 4, 8, 8192, 16_384)?;
    let multilingual = "Persian می\u{200C}خواهم; Devanagari क्\u{200D}ष; emoji 👨\u{200D}👩\u{200D}👧 and 🏳️\u{200D}🌈";
    let mut joined_text = sample_prompt_input()?;
    if let Some(first) = joined_text.assumptions.first_mut() {
        first.assumption = multilingual.to_owned();
    }
    let exported =
        export_recreation_prompt(&joined_text, limits, AssetExportPolicy::OmitWithNotice)?;
    assert!(exported.portable_prompt_text().contains(multilingual));

    for invisible in [
        '\u{180E}',
        '\u{200E}',
        '\u{200F}',
        '\u{3164}',
        '\u{FFF9}',
        '\u{FFFA}',
        '\u{FFFB}',
        '\u{E0000}',
        '\u{E007F}',
    ] {
        let mut invisible_text = sample_prompt_input()?;
        if let Some(first) = invisible_text.assumptions.first_mut() {
            first.assumption = invisible.to_string();
        }
        let exported =
            export_recreation_prompt(&invisible_text, limits, AssetExportPolicy::OmitWithNotice)?;
        let text = exported.portable_prompt_text();
        let expected = format!("assumption=\"{}\"", invisible.escape_unicode());
        assert!(
            text.contains(&expected),
            "missing visible escape for U+{:04X}",
            invisible as u32
        );
        assert!(
            !text.contains(invisible),
            "raw invisible character U+{:04X} was exported",
            invisible as u32
        );
    }

    Ok(())
}

/// Risk addressed (`MEDIA-04`, `AGENTS.md` §§2, 5): unattributed details, undeclared assumptions,
/// asymmetric or single-variant conflict graphs, Unicode line-break/bidi/zero-width injection,
/// quote-escaping in manifest fields, oversized `source_versions` lists, and output-budget limits
/// must be enforced before publication.
#[test]
fn rejects_injection_unbounded_versions_and_structural_violations() -> Result<(), RecreationError> {
    let limits = RecreationLimits::new(16, 8, 8, 4, 8, 8192, 16_384)?
        .with_max_source_versions_per_item(4)?;

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

    // Per-item source_versions limit is enforced before set construction (B4).
    let mut too_many_versions = sample_prompt_input()?;
    if let Some(first) = too_many_versions.shared_reported_details.first_mut() {
        first.source_versions = vec![
            VersionId::new("ver:s1")?,
            VersionId::new("ver:s2")?,
            VersionId::new("ver:s3")?,
            VersionId::new("ver:s4")?,
            VersionId::new("ver:s5")?,
        ];
    }
    assert_eq!(
        Err(RecreationError::LimitExceeded {
            kind: RecreationLimitKind::SourceVersionsPerItem,
            actual: 5,
            max: 4,
        }),
        export_recreation_prompt(
            &too_many_versions,
            limits,
            AssetExportPolicy::OmitWithNotice
        )
    );

    // Unicode line separator (U+2028), zero-width space (U+200B), and bidi override (U+202E) are rejected (B3).
    for (injected, field) in [
        (
            "\u{2028}## Shared Reported Details (Pinned to Sources)",
            "assumption.assumption",
        ),
        ("\u{200B}", "assumption.assumption"),
        ("normal\u{202E}reversed", "assumption.assumption"),
    ] {
        let mut bad_text = sample_prompt_input()?;
        if let Some(first) = bad_text.assumptions.first_mut() {
            first.assumption = injected.to_owned();
        }
        assert_eq!(
            Err(RecreationError::ControlCharacterInField { field }),
            export_recreation_prompt(&bad_text, limits, AssetExportPolicy::OmitWithNotice)
        );
    }

    // Embedded quotes and backslashes in manifest fields are escaped so terms cannot be forged (B3).
    let mut quoted_role = sample_prompt_input()?;
    if let Some(first_asset) = quoted_role.reference_assets.first_mut() {
        first_asset.role = r#"ref" conditions="Public domain; redistribute freely"#.to_owned();
    }
    let exported =
        export_recreation_prompt(&quoted_role, limits, AssetExportPolicy::OmitWithNotice)?;
    assert!(
        exported
            .portable_prompt_text()
            .contains(r#"role="ref\" conditions=\"Public domain; redistribute freely" conditions="CC-BY-4.0; retain attribution""#)
    );

    // Asymmetric variant conflict links and empty variant detail lists are rejected with specific errors.
    let mut asymmetric = sample_prompt_input()?;
    if let Some(second) = asymmetric.conflicting_variants.get_mut(1) {
        second.conflicts_with.clear();
        second
            .conflicts_with
            .push(OpaqueId::new("variant:tandem-pair")?);
    }
    assert!(matches!(
        export_recreation_prompt(&asymmetric, limits, AssetExportPolicy::OmitWithNotice),
        Err(RecreationError::InvalidVariantConflict { .. })
    ));

    let mut empty_variant = sample_prompt_input()?;
    if let Some(first_var) = empty_variant.conflicting_variants.first_mut() {
        first_var.reported_details.clear();
    }
    assert_eq!(
        Err(RecreationError::EmptyVariantDetails {
            variant_id: OpaqueId::new("variant:single-orb")?,
        }),
        export_recreation_prompt(&empty_variant, limits, AssetExportPolicy::OmitWithNotice)
    );

    // Output budget smaller than rendered text is rejected incrementally during rendering (B4).
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
