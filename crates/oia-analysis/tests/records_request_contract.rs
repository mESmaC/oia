use oia_analysis::records_request::{
    CustodianAssessment, DraftError, DraftInput, MAX_REFERENCES, MAX_REQUEST_BYTES,
    RecordsRequestDraft,
};
use oia_domain::VersionId;

/// INTEL-04/A08: preserve exact wording/uncertainty and bound untrusted input.
#[test]
fn draft_preserves_wording_and_rejects_invalid_input() -> Result<(), Box<dyn std::error::Error>> {
    let input = DraftInput {
        version: VersionId::new("draft-v1")?,
        custodian: "Example archive (synthetic)".into(),
        assessment: CustodianAssessment::Candidate,
        custodian_rationale: "Unverified lead from source-v2.".into(),
        request: "  Please supply café logs.\nKeep original wording.  ".into(),
        source_versions: vec![VersionId::new("source-v2")?],
    };
    let draft = RecordsRequestDraft::new(input.clone())?;
    assert_eq!(draft.input(), &input);
    let preview = draft.preview()?;
    assert!(preview.starts_with("DRAFT — NOT SUBMITTED\n"));
    for text in [&input.request, &input.custodian_rationale, &input.custodian] {
        assert!(preview.contains(text));
    }
    assert!(preview.contains("candidate; responsibility unconfirmed"));
    assert!(preview.contains("- source-v2\n"));
    let mut invalid = input.clone();
    invalid.request = "é".repeat(MAX_REQUEST_BYTES / 2 + 1);
    assert_eq!(
        RecordsRequestDraft::new(invalid),
        Err(DraftError::FieldTooLong("request"))
    );
    let mut invalid = input.clone();
    invalid.custodian_rationale = " \n".into();
    assert_eq!(
        RecordsRequestDraft::new(invalid),
        Err(DraftError::EmptyField("custodian rationale"))
    );
    let mut invalid = input.clone();
    invalid.request.push('\u{1b}');
    assert_eq!(
        RecordsRequestDraft::new(invalid),
        Err(DraftError::ControlCharacter("request"))
    );
    let mut invalid = input.clone();
    invalid.source_versions.push(VersionId::new("source-v2")?);
    assert_eq!(
        RecordsRequestDraft::new(invalid),
        Err(DraftError::DuplicateReference)
    );
    let mut invalid = input;
    invalid.source_versions = vec![VersionId::new("source-v2")?; MAX_REFERENCES + 1];
    assert_eq!(
        RecordsRequestDraft::new(invalid),
        Err(DraftError::TooManyReferences)
    );
    Ok(())
}
