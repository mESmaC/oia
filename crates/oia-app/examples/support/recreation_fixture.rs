use oia_analysis::statement_comparison::{
    CodedValue, ComparisonParameters, ComparisonRequest, DetailAssertion, ExplanationExposure,
    SourceClass, StatementSource, TextCapture,
};
use oia_app::recreation_composer::{CompositionRequest, ContextBindings, ContextChoice};
use oia_domain::witness::AccountOrigin;
use oia_domain::{ActorId, OpaqueId, VersionId};
use oia_media::recreation::{ExplicitAssumption, RecreationLimits, ReferenceAssetInput};
use std::error::Error;

const POLICE: &str = "A silent orange light hovered over the dark reservoir near the north shore for several minutes.";
const WITNESS: &str =
    "From the hill I noticed a red glow above the reservoir. I could not tell its size.";
const SIMULATED: &str = "The simulated object was blue with hypothetical triangular geometry.";

pub fn assertion(
    text: &'static str,
    aspect: &'static str,
    value: CodedValue<'static>,
    phrase: &str,
) -> Result<DetailAssertion<'static>, Box<dyn Error>> {
    let start = text.find(phrase).ok_or("missing fixture phrase")?;
    Ok(DetailAssertion {
        aspect,
        value,
        wording: start..start + phrase.len(),
    })
}

pub struct Fixture {
    police: Vec<DetailAssertion<'static>>,
    witness: Vec<DetailAssertion<'static>>,
    simulated: Vec<DetailAssertion<'static>>,
    police_lineage: [OpaqueId; 1],
    pub assumptions: Vec<ExplicitAssumption>,
    pub assets: Vec<ReferenceAssetInput>,
}

impl Fixture {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            police: vec![
                assertion(
                    POLICE,
                    "colour",
                    CodedValue::Reported("orange"),
                    "orange light",
                )?,
                assertion(POLICE, "sound", CodedValue::Reported("silent"), "silent")?,
                assertion(
                    POLICE,
                    "place",
                    CodedValue::Reported("reservoir"),
                    "over the dark reservoir",
                )?,
            ],
            witness: vec![
                assertion(WITNESS, "colour", CodedValue::Reported("red"), "red glow")?,
                assertion(
                    WITNESS,
                    "place",
                    CodedValue::Reported("reservoir"),
                    "above the reservoir",
                )?,
                assertion(
                    WITNESS,
                    "size",
                    CodedValue::StatedUnknown,
                    "I could not tell its size",
                )?,
            ],
            simulated: vec![
                assertion(SIMULATED, "colour", CodedValue::Reported("blue"), "blue")?,
                assertion(
                    SIMULATED,
                    "geometry",
                    CodedValue::Reported("triangular"),
                    "hypothetical triangular geometry",
                )?,
            ],
            police_lineage: [OpaqueId::new("account:police")?],
            assumptions: vec![ExplicitAssumption {
                assumption_id: OpaqueId::new("assumption:dusk")?,
                assumption: "Dusk framing for illustration only".to_owned(),
                rationale: "No coded capture time".to_owned(),
            }],
            assets: [true, false]
                .into_iter()
                .enumerate()
                .map(|(index, permitted)| {
                    Ok(ReferenceAssetInput {
                        asset_id: OpaqueId::new(format!("asset:photo-{index}"))?,
                        source_version: VersionId::new(format!("version:photo-{index}"))?,
                        sha256_hex: "a".repeat(64),
                        media_type: "image/png".to_owned(),
                        role: "Synthetic reference photograph declaration, not a loaded image"
                            .to_owned(),
                        redistribution_permitted: permitted,
                        usage_conditions:
                            "Fixture permission declaration; not independently verified".to_owned(),
                    })
                })
                .collect::<Result<Vec<_>, Box<dyn Error>>>()?,
        })
    }

    pub fn sources(&self) -> Result<Vec<StatementSource<'_>>, Box<dyn Error>> {
        Ok(vec![
            StatementSource {
                account: OpaqueId::new("account:police")?,
                revision: 0,
                version: VersionId::new("version:police-1")?,
                class: SourceClass::PoliceReport,
                origin: AccountOrigin::Reported,
                exposure: ExplanationExposure::Unknown,
                attribution: "Synthetic fixture standing in for a partial police report",
                text: TextCapture::Partial {
                    text: POLICE,
                    reason: "Only one paragraph captured",
                },
                assertions: &self.police,
                derived_from: &[],
            },
            StatementSource {
                account: OpaqueId::new("account:news")?,
                revision: 0,
                version: VersionId::new("version:news-1")?,
                class: SourceClass::NewsReport,
                origin: AccountOrigin::Reported,
                exposure: ExplanationExposure::Unknown,
                attribution: "Synthetic news fixture quoting the police",
                text: TextCapture::Complete(POLICE),
                assertions: &self.police,
                derived_from: &self.police_lineage,
            },
            StatementSource {
                account: OpaqueId::new("account:witness")?,
                revision: 0,
                version: VersionId::new("version:witness-1")?,
                class: SourceClass::Testimony,
                origin: AccountOrigin::Reported,
                exposure: ExplanationExposure::NotExposedInWorkflow,
                attribution: "Synthetic independent-account fixture",
                text: TextCapture::Complete(WITNESS),
                assertions: &self.witness,
                derived_from: &[],
            },
            StatementSource {
                account: OpaqueId::new("account:simulation")?,
                revision: 0,
                version: VersionId::new("version:simulation-1")?,
                class: SourceClass::Other,
                origin: AccountOrigin::Simulated,
                exposure: ExplanationExposure::Unknown,
                attribution: "Explicit simulated input",
                text: TextCapture::Complete(SIMULATED),
                assertions: &self.simulated,
                derived_from: &[],
            },
        ])
    }

    pub fn request<'a>(
        &'a self,
        sources: &'a [StatementSource<'a>],
        version: &str,
    ) -> Result<CompositionRequest<'a>, Box<dyn Error>> {
        Ok(CompositionRequest {
            comparison: ComparisonRequest {
                coding_version: VersionId::new("coding:fixture-1")?,
                coded_by: ActorId::new("actor:fixture")?,
                parameters: ComparisonParameters::default(),
                sources,
                dismissals: &[],
            },
            prompt_id: OpaqueId::new("prompt:fixture")?,
            prompt_version: VersionId::new(version)?,
            case_id: OpaqueId::new("case:fixture")?,
            title: "Synthetic mixed-source recreation fixture",
            context: ContextBindings {
                scene: ContextChoice::Aspect("place"),
                time: ContextChoice::Assumption(
                    &self
                        .assumptions
                        .first()
                        .ok_or("missing fixture assumption")?
                        .assumption_id,
                ),
                lighting: ContextChoice::Aspect("colour"),
                apparent_scale_aspect: Some("size"),
                ..ContextBindings::default()
            },
            required_aspects: &["distance"],
            assumptions: &self.assumptions,
            reference_assets: &self.assets,
            reason: "Compose retained fixture versions without inferred physical measurements",
        })
    }
}

pub fn limits() -> Result<RecreationLimits, Box<dyn Error>> {
    Ok(RecreationLimits::new(
        256, 32, 256, 256, 32, 262_144, 524_288,
    )?)
}
