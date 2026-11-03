//! Demonstrates exporting a portable illustrative recreation prompt from conflicting
//! synthetic witness accounts and validating an imported recreation (`MEDIA-04`, `A10`).
//!
//! All data in this example is a synthetic test fixture, not a real historical incident.

use std::error::Error;

use oia_domain::{OpaqueId, VersionId};
use oia_media::recreation::{
    AccountVariant, ApparentScale, AssetExportPolicy, AttemptedArtifactClass, ContextField,
    ExplicitAssumption, ExplicitUnknown, ImportedRecreationInput, RecreationLimits,
    RecreationPromptInput, ReferenceAssetInput, ReportedDetail, SceneContext,
    VISIBLE_RECREATION_LABEL, export_recreation_prompt, validate_imported_recreation,
};

fn main() -> Result<(), Box<dyn Error>> {
    let limits = RecreationLimits::new(32, 16, 16, 8, 16, 16_384, 32_768)?;

    let input = RecreationPromptInput {
        prompt_id: OpaqueId::new("prompt:synthetic-harbor-recreation")?,
        prompt_version: VersionId::new("ver:prompt-synthetic-harbor-v1")?,
        case_id: OpaqueId::new("case:synthetic-harbor-demo-01")?,
        title: "[SYNTHETIC FIXTURE] Harbor headland nocturnal sighting recreation".to_owned(),
        context: SceneContext {
            scene: ContextField::Reported {
                description: "Synthetic fixture: looking west toward outer harbor breakwater from North Pier".to_owned(),
                source_versions: vec![
                    VersionId::new("ver:synthetic-witness-a-v1")?,
                    VersionId::new("ver:synthetic-witness-b-v1")?,
                ],
            },
            time_context: ContextField::Reported {
                description: "2026-10-18 between 21:10 and 21:14 UTC (as logged by Pier Watch synthetic fixture)".to_owned(),
                source_versions: vec![VersionId::new("ver:synthetic-witness-a-v1")?],
            },
            viewpoint: ContextField::Reported {
                description: "Deck-level observer facing 265 deg azimuth, elevation ~8 deg above breakwater".to_owned(),
                source_versions: vec![VersionId::new("ver:synthetic-witness-a-v1")?],
            },
            lighting: ContextField::Assumed {
                description: "Overcast marine layer with faint amber harbor-beacon reflection on low clouds".to_owned(),
                assumption_id: OpaqueId::new("assumption:harbor-cloud-glow")?,
            },
            apparent_scale: ApparentScale::ReportedAngularOrRelative {
                description: "Apparent span reported as roughly two finger-widths at arm's length (~3 deg arc)".to_owned(),
                source_versions: vec![
                    VersionId::new("ver:synthetic-witness-a-v1")?,
                    VersionId::new("ver:synthetic-witness-b-v1")?,
                ],
                physical_size_unknown_reason: "Single-pier visual accounts without calibrated multi-station baseline leave slant range and physical span undetermined".to_owned(),
            },
        },
        shared_reported_details: vec![ReportedDetail {
            detail_id: OpaqueId::new("detail:steady-level-transit")?,
            statement: "Object crossed horizontally from north to south above the breakwater without altitude change".to_owned(),
            source_versions: vec![
                VersionId::new("ver:synthetic-witness-a-v1")?,
                VersionId::new("ver:synthetic-witness-b-v1")?,
            ],
            source_note: Some("Synthetic fixture: both accounts agree on level north-to-south path".to_owned()),
        }],
        conflicting_variants: vec![
            AccountVariant {
                variant_id: OpaqueId::new("variant:three-separated-lights")?,
                summary: "Three discrete white-amber point lights in a horizontal line with cloud texture visible between them".to_owned(),
                reported_details: vec![ReportedDetail {
                    detail_id: OpaqueId::new("detail:gaps-visible-between")?,
                    statement: "Observer with 7x50 marine binoculars reported background cloud texture visible between the three lights".to_owned(),
                    source_versions: vec![VersionId::new("ver:synthetic-witness-b-v1")?],
                    source_note: Some("Synthetic fixture Witness B statement lines 12-16".to_owned()),
                }],
                conflicts_with: vec![OpaqueId::new("variant:single-bar-silhouette")?],
            },
            AccountVariant {
                variant_id: OpaqueId::new("variant:single-bar-silhouette")?,
                summary: "Continuous dark rectangular bar silhouette occluding clouds with three recessed underside lamps".to_owned(),
                reported_details: vec![ReportedDetail {
                    detail_id: OpaqueId::new("detail:cloud-occlusion")?,
                    statement: "Unaided-eye observer reported a continuous dark bar blocking cloud glow between the three lights".to_owned(),
                    source_versions: vec![VersionId::new("ver:synthetic-witness-a-v1")?],
                    source_note: Some("Synthetic fixture Witness A statement lines 6-10".to_owned()),
                }],
                conflicts_with: vec![OpaqueId::new("variant:three-separated-lights")?],
            },
        ],
        assumptions: vec![ExplicitAssumption {
            assumption_id: OpaqueId::new("assumption:harbor-cloud-glow")?,
            assumption: "Low marine cloud deck rendered with diffuse harbor-beacon backlight".to_owned(),
            rationale: "Neither synthetic witness measured ceiling height; assumed for illustrative framing only".to_owned(),
        }],
        unknowns: vec![ExplicitUnknown {
            unknown_id: OpaqueId::new("unknown:slant-range-and-size")?,
            aspect: "True slant range, altitude MSL, and physical dimensions".to_owned(),
            impact_on_recreation: "Do not treat rendered perspective depth or object size as a physical measurement".to_owned(),
        }],
        reference_assets: vec![
            ReferenceAssetInput {
                asset_id: OpaqueId::new("asset:breakwater-profile-ref")?,
                source_version: VersionId::new("ver:synthetic-chart-profile-v1")?,
                sha256_hex: "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90".to_owned(),
                media_type: "image/png".to_owned(),
                role: "Synthetic breakwater horizon contour reference".to_owned(),
                redistribution_permitted: true,
                usage_conditions: "Synthetic public-domain fixture; retain source attribution".to_owned(),
            },
            ReferenceAssetInput {
                asset_id: OpaqueId::new("asset:witness-notebook-private")?,
                source_version: VersionId::new("ver:synthetic-witness-a-sketch-v1")?,
                sha256_hex: "0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c4b5a69788796a5b4c3d2e1f0".to_owned(),
                media_type: "image/png".to_owned(),
                role: "Internal witness notebook sketch".to_owned(),
                redistribution_permitted: false,
                usage_conditions: "Case-internal inspection only; redistribution not permitted".to_owned(),
            },
        ],
    };

    let exported = export_recreation_prompt(&input, limits, AssetExportPolicy::OmitWithNotice)?;
    print!("{}", exported.portable_prompt_text());

    let imported = validate_imported_recreation(
        &ImportedRecreationInput {
            artifact_id: OpaqueId::new("artifact:recreation-render-01")?,
            artifact_version: VersionId::new("ver:recreation-render-01-v1")?,
            sha256_hex: "11223344556677889900aabbccddeeff11223344556677889900aabbccddeeff"
                .to_owned(),
            media_type: "image/png".to_owned(),
            prompt_id: exported.prompt_id().clone(),
            prompt_version: exported.prompt_version().clone(),
            variant_id: Some(OpaqueId::new("variant:three-separated-lights")?),
            generator_model: "external-illustrator-v1".to_owned(),
            generator_settings: "seed=42, steps=30, cfg=6.5".to_owned(),
            attempted_class: AttemptedArtifactClass::IllustrativeRecreation,
            visible_label: VISIBLE_RECREATION_LABEL.to_owned(),
        },
        &exported,
        4096,
    )?;

    print!("\n{}", imported.portable_metadata_text(4096)?);

    Ok(())
}
