//! Demonstrates exporting a portable illustrative recreation prompt from conflicting
//! witness accounts and validating an imported recreation (`MEDIA-04`, `A10`).

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
        prompt_id: OpaqueId::new("prompt:phoenix-ridge-recreation")?,
        prompt_version: VersionId::new("ver:prompt-phoenix-ridge-v1")?,
        case_id: OpaqueId::new("case:phoenix-1997_01")?,
        title: "South Mountain ridge sighting illustrative recreation".to_owned(),
        context: SceneContext {
            scene: ContextField::Reported {
                description: "Looking south-southwest toward South Mountain ridge from suburban backyard".to_owned(),
                source_versions: vec![
                    VersionId::new("ver:witness-a-v1")?,
                    VersionId::new("ver:witness-b-v1")?,
                ],
            },
            time_context: ContextField::Reported {
                description: "1997-03-13 between 20:15 and 20:22 MST (03:15 to 03:22 UTC)".to_owned(),
                source_versions: vec![VersionId::new("ver:witness-a-v1")?],
            },
            viewpoint: ContextField::Reported {
                description: "Ground-level observer facing 205 deg azimuth, elevation ~12 deg above ridge".to_owned(),
                source_versions: vec![VersionId::new("ver:witness-a-v1")?],
            },
            lighting: ContextField::Assumed {
                description: "Clear post-twilight desert sky with faint urban sodium-vapor horizon glow".to_owned(),
                assumption_id: OpaqueId::new("assumption:urban-sky-glow")?,
            },
            apparent_scale: ApparentScale::ReportedAngularOrRelative {
                description: "Apparent span reported as roughly two fist-widths at arm's length (~20 deg arc)".to_owned(),
                source_versions: vec![
                    VersionId::new("ver:witness-a-v1")?,
                    VersionId::new("ver:witness-b-v1")?,
                ],
                physical_size_unknown_reason: "Single-corridor visual accounts without calibrated multi-station triangulation leave slant range and physical wingspan undetermined".to_owned(),
            },
        },
        shared_reported_details: vec![ReportedDetail {
            detail_id: OpaqueId::new("detail:silent-transit")?,
            statement: "No audible engine, rotor, or jet noise reported during overhead transit".to_owned(),
            source_versions: vec![
                VersionId::new("ver:witness-a-v1")?,
                VersionId::new("ver:witness-b-v1")?,
            ],
            source_note: Some("Both accounts explicitly note calm wind and absence of aircraft sound".to_owned()),
        }],
        conflicting_variants: vec![
            AccountVariant {
                variant_id: OpaqueId::new("variant:discrete-lights")?,
                summary: "Five discrete amber-white point lights in a symmetric V with background stars visible between lights".to_owned(),
                reported_details: vec![ReportedDetail {
                    detail_id: OpaqueId::new("detail:stars-visible-between")?,
                    statement: "Observer with 7x50 binoculars reported background stars visible between the five lights".to_owned(),
                    source_versions: vec![VersionId::new("ver:witness-b-v1")?],
                    source_note: Some("Passage lines 18-24 of Witness B statement".to_owned()),
                }],
                conflicts_with: vec![OpaqueId::new("variant:solid-chevron")?],
            },
            AccountVariant {
                variant_id: OpaqueId::new("variant:solid-chevron")?,
                summary: "Continuous matte-dark chevron silhouette occluding background stars with five recessed underside lights".to_owned(),
                reported_details: vec![ReportedDetail {
                    detail_id: OpaqueId::new("detail:star-occlusion")?,
                    statement: "Naked-eye observer reported a continuous dark leading edge blocking stars across the V span".to_owned(),
                    source_versions: vec![VersionId::new("ver:witness-a-v1")?],
                    source_note: Some("Passage lines 9-15 of Witness A statement".to_owned()),
                }],
                conflicts_with: vec![OpaqueId::new("variant:discrete-lights")?],
            },
        ],
        assumptions: vec![ExplicitAssumption {
            assumption_id: OpaqueId::new("assumption:urban-sky-glow")?,
            assumption: "Moderate suburban horizon skyglow rendered behind the ridge profile".to_owned(),
            rationale: "Neither witness measured sky brightness; assumed for illustrative framing only".to_owned(),
        }],
        unknowns: vec![ExplicitUnknown {
            unknown_id: OpaqueId::new("unknown:range-and-altitude")?,
            aspect: "True slant range, altitude AGL/MSL, and physical structure dimensions".to_owned(),
            impact_on_recreation: "Do not treat rendered perspective depth or object size as a physical measurement".to_owned(),
        }],
        reference_assets: vec![
            ReferenceAssetInput {
                asset_id: OpaqueId::new("asset:ridge-silhouette-ref")?,
                source_version: VersionId::new("ver:usgs-dem-profile-v1")?,
                sha256_hex: "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90".to_owned(),
                media_type: "image/png".to_owned(),
                role: "Horizon terrain profile reference".to_owned(),
                redistribution_permitted: true,
                usage_conditions: "Public domain elevation profile; retain source attribution".to_owned(),
            },
            ReferenceAssetInput {
                asset_id: OpaqueId::new("asset:witness-sketch-private")?,
                source_version: VersionId::new("ver:witness-a-sketch-v1")?,
                sha256_hex: "0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c4b5a69788796a5b4c3d2e1f0".to_owned(),
                media_type: "image/png".to_owned(),
                role: "Internal witness pencil sketch".to_owned(),
                redistribution_permitted: false,
                usage_conditions: "Case-internal inspection only; redistribution not permitted".to_owned(),
            },
        ],
    };

    let exported = export_recreation_prompt(&input, limits, AssetExportPolicy::OmitWithNotice)?;
    print!("{}", exported.portable_prompt_text);

    let imported = validate_imported_recreation(
        &ImportedRecreationInput {
            artifact_id: OpaqueId::new("artifact:recreation-render-01")?,
            artifact_version: VersionId::new("ver:recreation-render-01-v1")?,
            sha256_hex: "11223344556677889900aabbccddeeff11223344556677889900aabbccddeeff"
                .to_owned(),
            media_type: "image/png".to_owned(),
            prompt_id: exported.prompt_id.clone(),
            prompt_version: exported.prompt_version.clone(),
            variant_id: Some(OpaqueId::new("variant:discrete-lights")?),
            generator_model: "external-illustrator-v1".to_owned(),
            generator_settings: "seed=42, steps=30, cfg=6.5".to_owned(),
            attempted_class: AttemptedArtifactClass::IllustrativeRecreation,
            visible_label: VISIBLE_RECREATION_LABEL.to_owned(),
        },
        &exported,
        4096,
    )?;

    println!(
        "\nValidated imported recreation `{}` (variant={:?}, observation_evidence={})",
        imported.artifact_id,
        imported
            .variant_id
            .as_ref()
            .map(OpaqueId::as_str)
            .unwrap_or("shared"),
        imported.can_serve_as_observation_evidence
    );

    Ok(())
}
