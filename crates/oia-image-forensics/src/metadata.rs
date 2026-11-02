use crate::{Cue, ForensicError, MetadataAnalysis, UninspectedContainer, digest};

fn skipped(
    result: &mut MetadataAnalysis,
    container: String,
    offset: usize,
    length: usize,
    reason: &'static str,
) {
    result.uninspected.push(UninspectedContainer {
        container,
        byte_offset: offset,
        byte_length: length,
        reason,
    });
}

fn split_null(bytes: &[u8], offset: usize) -> Result<(&[u8], &[u8]), ForensicError> {
    let position = bytes
        .iter()
        .position(|value| *value == 0)
        .ok_or(ForensicError::Malformed {
            offset,
            reason: "missing PNG text separator",
        })?;
    let before = bytes.get(..position).ok_or(ForensicError::Malformed {
        offset,
        reason: "text field",
    })?;
    let after = bytes.get(position + 1..).ok_or(ForensicError::Malformed {
        offset,
        reason: "text field",
    })?;
    Ok((before, after))
}

pub(crate) fn png_field(
    result: &mut MetadataAnalysis,
    kind: &[u8],
    payload: &[u8],
    offset: usize,
) -> Result<(), ForensicError> {
    if kind != b"tEXt" && kind != b"iTXt" {
        skipped(
            result,
            String::from_utf8_lossy(kind).into_owned(),
            offset,
            payload.len(),
            "container semantics not inspected; compressed text, Exif, credentials and profiles are not decoded",
        );
        return Ok(());
    }
    let (key, remainder) = split_null(payload, offset)?;
    if key.is_empty() || key.len() > 79 {
        return Err(ForensicError::Malformed {
            offset,
            reason: "invalid PNG text keyword length",
        });
    }
    let text = if kind == b"iTXt" {
        let flag = remainder.first().copied();
        if !matches!(flag, Some(0 | 1)) || remainder.get(1) != Some(&0) {
            return Err(ForensicError::Malformed {
                offset,
                reason: "invalid iTXt compression header",
            });
        }
        let fields = remainder.get(2..).ok_or(ForensicError::Malformed {
            offset,
            reason: "truncated iTXt",
        })?;
        let (_, translated) = split_null(fields, offset)?;
        let (_, text) = split_null(translated, offset)?;
        if flag == Some(1) {
            skipped(
                result,
                "iTXt".to_owned(),
                offset,
                payload.len(),
                "compressed text deliberately not decompressed",
            );
            return Ok(());
        }
        if std::str::from_utf8(text).is_err() {
            return Err(ForensicError::Malformed {
                offset,
                reason: "iTXt text is not UTF-8",
            });
        }
        text
    } else {
        if remainder.contains(&0) {
            return Err(ForensicError::Malformed {
                offset,
                reason: "embedded null in tEXt",
            });
        }
        remainder
    };
    inspect_text(
        result,
        key,
        text,
        if kind == b"tEXt" {
            "PNG tEXt"
        } else {
            "PNG iTXt"
        },
        payload,
        offset,
    );
    Ok(())
}

pub(crate) fn jpeg_field(result: &mut MetadataAnalysis, marker: u8, payload: &[u8], offset: usize) {
    let xmp_header = b"http://ns.adobe.com/xap/1.0/\0";
    if marker == 0xfe {
        inspect_text(result, b"Comment", payload, "JPEG COM", payload, offset);
    } else if marker == 0xe1 && payload.starts_with(xmp_header) {
        if let Some(text) = payload.get(xmp_header.len()..) {
            inspect_text(result, b"XMP", text, "JPEG APP1 XMP", payload, offset);
        }
    } else if marker != 0xe0 && marker != 0xee {
        skipped(
            result,
            format!("JPEG marker FF{marker:02X}"),
            offset,
            payload.len(),
            "opaque application metadata; no Exif, extended XMP, ICC or C2PA validation",
        );
    }
}

fn inspect_text(
    result: &mut MetadataAnalysis,
    key: &[u8],
    text: &[u8],
    container: &'static str,
    payload: &[u8],
    offset: usize,
) {
    result.inspected_text_fields += 1;
    let lowered = String::from_utf8_lossy(text).to_ascii_lowercase();
    let mut found = Vec::new();
    let mut add = |kind, interpretation| found.push((kind, interpretation));
    if (key.eq_ignore_ascii_case(b"parameters") || key == b"Comment")
        && ["steps:", "sampler:", "seed:"]
            .iter()
            .all(|token| lowered.contains(token))
    {
        add(
            "generation_parameters_signature",
            "Text resembles diffusion generation parameters; this is a forgeable declaration, not pixel-level AI detection.",
        );
    }
    if key.eq_ignore_ascii_case(b"prompt") {
        match serde_json::from_slice::<serde_json::Value>(text) {
            Ok(serde_json::Value::Object(nodes)) => {
                if nodes.values().any(|node| {
                    matches!(
                        node.get("class_type").and_then(serde_json::Value::as_str),
                        Some(
                            "KSampler"
                                | "KSamplerAdvanced"
                                | "SamplerCustom"
                                | "SamplerCustomAdvanced"
                        )
                    )
                }) {
                    add(
                        "generation_workflow_signature",
                        "JSON contains a ComfyUI sampler node declaration; it does not verify execution or image origin.",
                    );
                }
            }
            _ => skipped(
                result,
                container.to_owned(),
                offset,
                payload.len(),
                "prompt is not a supported JSON object; invalid, overly nested or other generation schemas remain uninspected",
            ),
        }
    }
    if (key.eq_ignore_ascii_case(b"Software")
        || key == b"XMP"
        || key == b"Comment"
        || key.eq_ignore_ascii_case(b"XML:com.adobe.xmp"))
        && [
            "adobe photoshop",
            "gimp",
            "adobe lightroom",
            "affinity photo",
        ]
        .iter()
        .any(|token| lowered.contains(token))
    {
        add(
            "editing_software_signature",
            "Metadata mentions editing software; ordinary processing or a textual mention can produce the same cue.",
        );
    }
    if (key == b"XMP" || key.eq_ignore_ascii_case(b"XML:com.adobe.xmp"))
        && [
            "http://cv.iptc.org/newscodes/digitalsourcetype/trainedalgorithmicmedia",
            "http://cv.iptc.org/newscodes/digitalsourcetype/compositewithtrainedalgorithmicmedia",
        ]
        .iter()
        .any(|token| lowered.contains(token))
    {
        add(
            "synthetic_source_type_signature",
            "XMP text contains an IPTC synthetic source-type URI; XML structure, signatures, trust and pixel binding are not verified.",
        );
    }
    for (kind, interpretation) in found {
        result.cues.push(Cue {
            kind,
            container,
            byte_offset: offset,
            byte_length: payload.len(),
            payload_sha256: digest(payload),
            interpretation,
        });
    }
}
