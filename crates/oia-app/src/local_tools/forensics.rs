//! Still-image forensic cues over the held image snapshot (`MEDIA-01`, `#30` GUI slice).
//! The merged `oia-image-forensics` kernel runs on the same retained bytes the Privacy
//! instrument inspects; it never changes them, and its report is passed to the page as
//! the kernel serializes it, so every method's coverage, scope and limitations reach the
//! inspector unedited. The assessment is the kernel's: inconclusive and unreviewed.
use super::{Control, FileInfo, FileKind, IMAGE_BYTES, Result, Snapshot, ToolError, require_kind};
use oia_image_forensics::{Config, ForensicError, Report, StopReason, analyze};
use serde::Serialize;

/// The URI scheme through which the page reads the held image's bytes (Rust-owned,
/// read-only, keyed by the snapshot hash). On Windows the WebView addresses it as
/// `http://oia-snapshot.localhost/<sha256>`; elsewhere as `oia-snapshot://localhost/<sha256>`.
pub const SNAPSHOT_SCHEME: &str = "oia-snapshot";

#[derive(Debug, Serialize)]
pub struct ForensicView {
    pub file: FileInfo,
    /// Path under the snapshot scheme that serves the original bytes for display.
    pub image_path: String,
    pub media_type: &'static str,
    /// True when any method reports partial coverage (the kernel's own rule).
    pub partial: bool,
    pub report: Report,
}

/// Media type of a held image snapshot by its leading bytes; `None` if neither container.
pub fn media_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xff, 0xd8]) {
        Some("image/jpeg")
    } else {
        None
    }
}

fn config() -> Config {
    Config {
        max_input_bytes: IMAGE_BYTES,
        // The operation deadline (30 s) is enforced by `Control`; the kernel's own
        // deadline stays at its default so the two agree.
        emit_residual_map: false,
        ..Config::default()
    }
}

fn map_error(error: ForensicError) -> ToolError {
    match error {
        ForensicError::Stopped(StopReason::Cancelled) => {
            ToolError::new("cancelled", "Operation cancelled; no result applied.")
        }
        ForensicError::Stopped(_) => ToolError::new(
            "deadline",
            "Forensic analysis exceeded its processing limit; no result applied.",
        ),
        ForensicError::Limit(limit) => ToolError::new(
            "forensic_limit",
            format!(
                "Forensic analysis stopped at a resource limit ({limit}); nothing was truncated silently."
            ),
        ),
        ForensicError::Unsupported(reason) => ToolError::new(
            "forensic_unsupported",
            format!("This image is outside the supported forensic profile: {reason}."),
        ),
        ForensicError::Malformed { offset, reason } => ToolError::new(
            "forensic_malformed",
            format!("Malformed image at byte {offset}: {reason}."),
        ),
        ForensicError::InvalidConfig => {
            ToolError::new("internal", "The forensic configuration is invalid.")
        }
        other => ToolError::new("forensic_decode", other.to_string()),
    }
}

/// Runs the forensic profile on the held image. Bounded by the kernel's limits and the
/// operation control; a stop leaves no partial report behind.
pub fn analyze_image(snapshot: &Snapshot, control: &Control) -> Result<ForensicView> {
    require_kind(snapshot, FileKind::Image)?;
    let media_type = media_type(snapshot.bytes()).ok_or_else(|| {
        ToolError::new(
            "forensic_unsupported",
            "Only PNG and JPEG containers are analysed.",
        )
    })?;
    let stop = || {
        control.check().map_err(|e| {
            if e.code == "cancelled" {
                StopReason::Cancelled
            } else {
                StopReason::DeadlineExceeded
            }
        })
    };
    let analysis = analyze(snapshot.bytes(), &config(), stop).map_err(map_error)?;
    control.check()?;
    let partial = analysis.report.profile_is_partial();
    Ok(ForensicView {
        file: snapshot.info.clone(),
        image_path: format!("/{}", snapshot.info.sha256),
        media_type,
        partial,
        report: analysis.report,
    })
}

/// Produce the kernel's labeled diagnostic PNG from the retained JPEG bytes.
/// This is a conventional derivative; JPEG decoder coverage remains partial.
pub fn residual_map(snapshot: &Snapshot, control: &Control) -> Result<Vec<u8>> {
    require_kind(snapshot, FileKind::Image)?;
    control.check()?;
    if media_type(snapshot.bytes()) != Some("image/jpeg") {
        return Err(ToolError::new(
            "forensic_unsupported",
            "Residual maps require a JPEG image. PNG recompression residuals are not applicable.",
        ));
    }
    let mut settings = config();
    settings.emit_residual_map = true;
    let analysis = analyze(snapshot.bytes(), &settings, || {
        control.check().map_err(|e| {
            if e.code == "cancelled" {
                StopReason::Cancelled
            } else {
                StopReason::DeadlineExceeded
            }
        })
    })
    .map_err(map_error)?;
    control.check()?;
    analysis.residual_png.ok_or_else(|| {
        ToolError::new(
            "forensic_unsupported",
            "The kernel produced no residual map; nothing was saved.",
        )
    })
}
