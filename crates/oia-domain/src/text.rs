//! Unicode classification for retained-text display and structural validation.
//!
//! Renderers preserve language and emoji joiners in prose. Structural fields may
//! reject those same characters. This is an explicit character policy, not a
//! complete Unicode confusable detector, sanitizer, or evidence transformation.

/// Whether a formatting character needs special treatment in structural fields.
/// Includes ZWNJ/ZWJ; callers decide whether to reject or visibly escape it.
pub fn is_format_control(ch: char) -> bool {
    matches!(
        ch,
        '\u{0085}'
            | '\u{00AD}'
            | '\u{0600}'..='\u{0605}'
            | '\u{061C}'
            | '\u{06DD}'
            | '\u{070F}'
            | '\u{0890}'..='\u{0891}'
            | '\u{08E2}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{2028}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
            | '\u{3164}'
            | '\u{FEFF}'
            | '\u{FFA0}'
            | '\u{FFF9}'..='\u{FFFB}'
            | '\u{110BD}'
            | '\u{110CD}'
            | '\u{13430}'..='\u{1343F}'
            | '\u{1BCA0}'..='\u{1BCA3}'
            | '\u{1D173}'..='\u{1D17A}'
            | '\u{E0000}'..='\u{E007F}'
    )
}

/// Whether prose renderers should show a character as a visible Unicode escape.
/// ZWNJ (U+200C) and ZWJ (U+200D) remain verbatim for language and emoji shaping.
/// Controls are included; renderers may handle permitted line breaks separately.
pub fn requires_visible_unicode_escape(ch: char) -> bool {
    ch.is_control() || (is_format_control(ch) && !matches!(ch, '\u{200C}' | '\u{200D}'))
}
