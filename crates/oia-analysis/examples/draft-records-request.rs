//! Synthetic local preview; no files, network, mail, or submission.
use oia_analysis::records_request::{CustodianAssessment, DraftInput, RecordsRequestDraft};
use oia_domain::VersionId;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let draft = RecordsRequestDraft::new(DraftInput {
        version: VersionId::new("synthetic-draft-v1")?,
        custodian: "Example Observatory records office (synthetic)".into(),
        assessment: CustodianAssessment::Candidate,
        custodian_rationale: "Synthetic research lead; no real custodian has been verified.".into(),
        request: "Please provide the publicly releasable instrument maintenance logs for 2025.\nIf another office holds these records, please identify that office.".into(),
        source_versions: vec![],
    })?;
    print!("{}", draft.preview()?);
    Ok(())
}
