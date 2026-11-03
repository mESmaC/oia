//! Critical app contracts: source snapshots/anchors, visible bounds, non-destructive export.
use oia_app::local_tools::*;
use std::{error::Error, fs};

#[test]
fn searches_the_opened_bytes_even_after_the_source_changes()
-> std::result::Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("source.txt");
    let original = "évidence\r\n<script>untrusted</script> two lights\n";
    fs::write(&path, original)?;
    let control = Control::default();
    let snapshot = open_file(&path, FileKind::Text, &control)?;
    let view = document(&snapshot, &control)?;
    assert!(view.complete);
    assert_eq!(view.passages[1].start, "évidence\r\n".len());
    fs::write(&path, "later revision")?;
    let result = search(&snapshot, "two lights", &control)?;
    assert_eq!(result.matches.len(), 1);
    let hit = &result.matches[0];
    assert_eq!(&original.as_bytes()[hit.start..hit.end], b"two lights");
    assert_eq!(result.sha256, snapshot.info.sha256);
    assert_eq!(snapshot.bytes(), original.as_bytes());
    assert_eq!(fs::read_to_string(path)?, "later revision");
    Ok(())
}

#[test]
fn bounds_invalid_input_and_cancellation_are_visible() -> std::result::Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("source.txt");
    fs::write(&path, "match\n".repeat(PASSAGES + 1))?;
    let control = Control::default();
    let snapshot = open_file(&path, FileKind::Text, &control)?;
    assert!(!document(&snapshot, &control)?.complete);
    let result = search(&snapshot, "match", &control)?;
    assert!(!result.complete);
    assert_eq!(result.matches.len(), MATCHES);
    assert!(result.searched_bytes < result.input_bytes);
    assert!(search(&snapshot, "", &control).is_err());
    control.cancel();
    assert!(search(&snapshot, "match", &control).is_err());
    fs::write(&path, [0xff])?;
    assert!(
        document(
            &open_file(&path, FileKind::Text, &Control::default())?,
            &Control::default()
        )
        .is_err()
    );
    fs::File::create(&path)?.set_len(TEXT_BYTES as u64 + 1)?;
    assert_eq!(
        open_file(&path, FileKind::Text, &Control::default())
            .err()
            .map(|e| e.code),
        Some("input_limit")
    );
    Ok(())
}

fn decode(hex: &str) -> std::result::Result<Vec<u8>, Box<dyn Error>> {
    hex.trim()
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}

#[test]
fn cleaned_copy_is_reinspected_and_never_replaces_an_existing_file()
-> std::result::Result<(), Box<dyn Error>> {
    let fixtures = [
        (
            "source.png",
            include_str!("../../oia-png-privacy/tests/fixtures/rgba-exif.hex"),
        ),
        (
            "source.jpg",
            include_str!("../../oia-jpeg-privacy/tests/fixtures/baseline.hex"),
        ),
    ];
    let dir = tempfile::tempdir()?;
    for (name, fixture) in fixtures {
        let original = decode(fixture)?;
        let path = dir.path().join(name);
        fs::write(&path, &original)?;
        let control = Control::default();
        let snapshot = open_file(&path, FileKind::Image, &control)?;
        let selection = Selection {
            text: true,
            modification_time: true,
            exif: true,
            comment: true,
            xmp: true,
            photoshop_irb: true,
        };
        let cleaned = clean_image(&snapshot, &selection, &control)?;
        assert!(cleaned.removed > 0);
        assert!(PreparedExport::prepare(&path, &cleaned.bytes, &control).is_err());
        let output = dir.path().join(format!("cleaned-{name}"));
        let staged = PreparedExport::prepare(&output, &cleaned.bytes, &control)?;
        // An output created after staging is preserved as well.
        fs::write(&output, "another session")?;
        assert!(staged.commit(&control).is_err());
        assert_eq!(fs::read_to_string(&output)?, "another session");
        let output = dir.path().join(format!("result-{name}"));
        PreparedExport::prepare(&output, &cleaned.bytes, &control)?.commit(&control)?;
        let inspection = inspect_image(&open_file(&output, FileKind::Image, &control)?, &control)?;
        assert!(inspection.complete);
        assert!(
            inspection
                .rows
                .iter()
                .all(|row| row.handling != "removable")
        );
        assert_eq!(fs::read(&path)?, original);
        let cancelled_path = dir.path().join(format!("cancelled-{name}"));
        let staged = PreparedExport::prepare(&cancelled_path, &cleaned.bytes, &control)?;
        control.cancel();
        assert!(staged.commit(&control).is_err());
        assert!(!cancelled_path.exists());
    }
    Ok(())
}

// MEDIA-01/A10: the desktop adapter must keep the kernel's derivative labels,
// retained source identity, uncertainty and non-destructive publication behavior.
#[test]
fn residual_export_preserves_labels_and_source() -> std::result::Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("synthetic.jpg");
    let original = decode(include_str!("fixtures/synthetic-texture-jpeg.hex"))?;
    fs::write(&path, &original)?;
    let control = Control::default();
    let snapshot = open_file(&path, FileKind::Image, &control)?;
    fs::write(&path, "later source revision")?;
    let bytes = residual_map(&snapshot, &control)?;
    let contains = |text: &str| bytes.windows(text.len()).any(|b| b == text.as_bytes());
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert!(contains(&snapshot.info.sha256));
    assert!(contains(
        "derived JPEG recompression residual; not an observation or authenticity verdict"
    ));
    assert!(contains(
        "JPEG entropy completeness unverified; decoder recovery possible"
    ));
    assert!(contains("quality=90,gain=8"));
    assert_eq!(snapshot.bytes(), original);
    assert_eq!(fs::read_to_string(&path)?, "later source revision");
    assert!(PreparedExport::prepare(&path, &bytes, &control).is_err());
    let destination = dir.path().join("diagnostic.png");
    PreparedExport::prepare(&destination, &bytes, &control)?.commit(&control)?;
    assert_eq!(fs::read(&destination)?, bytes);
    let png = open_file(&destination, FileKind::Image, &control)?;
    assert_eq!(
        residual_map(&png, &control).err().map(|e| e.code),
        Some("forensic_unsupported")
    );
    let cancelled = dir.path().join("cancelled.png");
    let staged = PreparedExport::prepare(&cancelled, &bytes, &control)?;
    control.cancel();
    assert!(staged.commit(&control).is_err());
    assert!(!cancelled.exists());
    assert_eq!(
        residual_map(&snapshot, &control).err().map(|e| e.code),
        Some("cancelled")
    );
    Ok(())
}
