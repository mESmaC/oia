//! Resource-bounded JSON parser and canonical serializer.
//!
//! Enforces explicit size, nesting-depth, container-length, and string-length budgets
//! before allocation, and rejects non-finite floating-point values at persistence and
//! transport boundaries (`AGENTS.md` Sections 4 and 5).

use std::collections::BTreeMap;
use std::fmt;

/// Resource limits enforced while parsing untrusted JSON payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JsonBudget {
    /// Maximum total input payload size in bytes.
    pub max_input_bytes: usize,
    /// Maximum structural nesting depth for arrays and objects.
    pub max_depth: usize,
    /// Maximum number of elements in a single array or entries in a single object.
    pub max_container_items: usize,
    /// Maximum decoded byte length of a single string value or key.
    pub max_string_bytes: usize,
}

impl Default for JsonBudget {
    fn default() -> Self {
        Self {
            max_input_bytes: 4 * 1024 * 1024,
            max_depth: 32,
            max_container_items: 10_000,
            max_string_bytes: 256 * 1024,
        }
    }
}

/// Canonical JSON AST value with deterministic `BTreeMap` object key ordering.
#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    /// JSON `null`.
    Null,
    /// JSON `true` or `false`.
    Bool(bool),
    /// Exact signed 64-bit integer.
    Int(i64),
    /// Validated finite IEEE-754 float (`NaN` and `Infinity` are rejected).
    Float(f64),
    /// UTF-8 string.
    Str(String),
    /// Ordered JSON array.
    Array(Vec<JsonValue>),
    /// Deterministically sorted JSON object (`BTreeMap`).
    Object(BTreeMap<String, JsonValue>),
}

/// Errors produced when parsing or validating JSON payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonError {
    /// Input exceeds `JsonBudget::max_input_bytes`.
    InputTooLarge {
        /// Actual byte count.
        actual: usize,
        /// Configured maximum byte count.
        max: usize,
    },
    /// Nesting exceeds `JsonBudget::max_depth`.
    MaxDepthExceeded {
        /// Configured maximum depth.
        max: usize,
    },
    /// Array or object exceeds `JsonBudget::max_container_items`.
    TooManyContainerItems {
        /// Configured maximum item count.
        max: usize,
    },
    /// Decoded string exceeds `JsonBudget::max_string_bytes`.
    StringTooLong {
        /// Configured maximum string byte length.
        max: usize,
    },
    /// Non-finite float (`NaN` or `Infinity`) rejected at boundary.
    NonFiniteFloat,
    /// Duplicate object key encountered in strict mode.
    DuplicateObjectKey(String),
    /// Syntax error at the given byte offset.
    Syntax {
        /// Byte offset in input.
        offset: usize,
        /// Explanation of syntax failure.
        reason: &'static str,
    },
    /// Required field was absent or had an unexpected type during schema decoding.
    Schema(String),
}

impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputTooLarge { actual, max } => {
                write!(f, "JSON payload size {actual} exceeds limit {max} bytes")
            }
            Self::MaxDepthExceeded { max } => {
                write!(f, "JSON nesting depth exceeds limit {max}")
            }
            Self::TooManyContainerItems { max } => {
                write!(f, "JSON container item count exceeds limit {max}")
            }
            Self::StringTooLong { max } => {
                write!(f, "JSON string length exceeds limit {max} bytes")
            }
            Self::NonFiniteFloat => write!(f, "non-finite floating-point value is not permitted"),
            Self::DuplicateObjectKey(k) => write!(f, "duplicate JSON object key '{k}'"),
            Self::Syntax { offset, reason } => {
                write!(f, "JSON syntax error at byte {offset}: {reason}")
            }
            Self::Schema(msg) => write!(f, "JSON schema validation error: {msg}"),
        }
    }
}

impl std::error::Error for JsonError {}

impl JsonValue {
    /// Constructs a validated finite float value, rejecting `NaN` and `Infinity`.
    pub fn finite_float(value: f64) -> Result<Self, JsonError> {
        if !value.is_finite() {
            return Err(JsonError::NonFiniteFloat);
        }
        Ok(Self::Float(value))
    }

    /// Parses `input` under the default [`JsonBudget`].
    pub fn parse(input: &str) -> Result<Self, JsonError> {
        Self::parse_bounded(input, JsonBudget::default())
    }

    /// Parses `input` while strictly enforcing `budget`.
    pub fn parse_bounded(input: &str, budget: JsonBudget) -> Result<Self, JsonError> {
        if input.len() > budget.max_input_bytes {
            return Err(JsonError::InputTooLarge {
                actual: input.len(),
                max: budget.max_input_bytes,
            });
        }
        let mut parser = Parser {
            bytes: input.as_bytes(),
            pos: 0,
            budget,
        };
        let value = parser.parse_value(0)?;
        parser.skip_ws();
        if parser.pos != parser.bytes.len() {
            return Err(JsonError::Syntax {
                offset: parser.pos,
                reason: "unexpected trailing characters after root value",
            });
        }
        Ok(value)
    }

    /// Serializes this value into deterministic, canonical compact JSON.
    pub fn to_canonical_json(&self) -> Result<String, JsonError> {
        let mut out = String::new();
        self.write_canonical(&mut out)?;
        Ok(out)
    }

    fn write_canonical(&self, out: &mut String) -> Result<(), JsonError> {
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(true) => out.push_str("true"),
            Self::Bool(false) => out.push_str("false"),
            Self::Int(n) => out.push_str(&n.to_string()),
            Self::Float(v) => {
                if !v.is_finite() {
                    return Err(JsonError::NonFiniteFloat);
                }
                let s = v.to_string();
                out.push_str(&s);
                if !s.contains('.') && !s.contains('e') && !s.contains('E') {
                    out.push_str(".0");
                }
            }
            Self::Str(s) => write_escaped_str(s, out),
            Self::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.write_canonical(out)?;
                }
                out.push(']');
            }
            Self::Object(map) => {
                out.push('{');
                for (i, (k, v)) in map.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_escaped_str(k, out);
                    out.push(':');
                    v.write_canonical(out)?;
                }
                out.push('}');
            }
        }
        Ok(())
    }

    /// Returns this value as an object map, or a schema error.
    pub fn as_object(&self) -> Result<&BTreeMap<String, JsonValue>, JsonError> {
        match self {
            Self::Object(m) => Ok(m),
            _ => Err(JsonError::Schema("expected JSON object".to_owned())),
        }
    }

    /// Returns a required string field from an object.
    pub fn req_str<'a>(
        obj: &'a BTreeMap<String, JsonValue>,
        key: &str,
    ) -> Result<&'a str, JsonError> {
        match obj.get(key) {
            Some(Self::Str(s)) => Ok(s.as_str()),
            Some(_) => Err(JsonError::Schema(format!("field '{key}' must be a string"))),
            None => Err(JsonError::Schema(format!(
                "missing required string field '{key}'"
            ))),
        }
    }

    /// Returns an optional string field from an object.
    pub fn opt_str(
        obj: &BTreeMap<String, JsonValue>,
        key: &str,
    ) -> Result<Option<String>, JsonError> {
        match obj.get(key) {
            None | Some(Self::Null) => Ok(None),
            Some(Self::Str(s)) => Ok(Some(s.clone())),
            Some(_) => Err(JsonError::Schema(format!(
                "field '{key}' must be a string or null"
            ))),
        }
    }

    /// Returns a required integer field from an object.
    pub fn req_i64(obj: &BTreeMap<String, JsonValue>, key: &str) -> Result<i64, JsonError> {
        match obj.get(key) {
            Some(Self::Int(n)) => Ok(*n),
            Some(_) => Err(JsonError::Schema(format!(
                "field '{key}' must be an integer"
            ))),
            None => Err(JsonError::Schema(format!(
                "missing required integer field '{key}'"
            ))),
        }
    }

    /// Returns a required boolean field from an object.
    pub fn req_bool(obj: &BTreeMap<String, JsonValue>, key: &str) -> Result<bool, JsonError> {
        match obj.get(key) {
            Some(Self::Bool(b)) => Ok(*b),
            Some(_) => Err(JsonError::Schema(format!(
                "field '{key}' must be a boolean"
            ))),
            None => Err(JsonError::Schema(format!(
                "missing required boolean field '{key}'"
            ))),
        }
    }

    /// Returns a required array field from an object.
    pub fn req_array<'a>(
        obj: &'a BTreeMap<String, JsonValue>,
        key: &str,
    ) -> Result<&'a [JsonValue], JsonError> {
        match obj.get(key) {
            Some(Self::Array(arr)) => Ok(arr.as_slice()),
            Some(_) => Err(JsonError::Schema(format!("field '{key}' must be an array"))),
            None => Err(JsonError::Schema(format!(
                "missing required array field '{key}'"
            ))),
        }
    }

    /// Returns an optional integer field from an object.
    pub fn opt_i64(obj: &BTreeMap<String, JsonValue>, key: &str) -> Result<Option<i64>, JsonError> {
        match obj.get(key) {
            None | Some(Self::Null) => Ok(None),
            Some(Self::Int(n)) => Ok(Some(*n)),
            Some(_) => Err(JsonError::Schema(format!(
                "field '{key}' must be an integer or null"
            ))),
        }
    }

    /// Returns a required finite float field from an object.
    pub fn req_f64(obj: &BTreeMap<String, JsonValue>, key: &str) -> Result<f64, JsonError> {
        match obj.get(key) {
            Some(Self::Float(v)) if v.is_finite() => Ok(*v),
            Some(Self::Int(n)) => Ok(*n as f64),
            Some(_) => Err(JsonError::Schema(format!(
                "field '{key}' must be a finite number"
            ))),
            None => Err(JsonError::Schema(format!(
                "missing required number field '{key}'"
            ))),
        }
    }

    /// Returns an optional finite float field from an object.
    pub fn opt_f64(obj: &BTreeMap<String, JsonValue>, key: &str) -> Result<Option<f64>, JsonError> {
        match obj.get(key) {
            None | Some(Self::Null) => Ok(None),
            Some(Self::Float(v)) if v.is_finite() => Ok(Some(*v)),
            Some(Self::Int(n)) => Ok(Some(*n as f64)),
            Some(_) => Err(JsonError::Schema(format!(
                "field '{key}' must be a finite number or null"
            ))),
        }
    }

    /// Returns a required object field from an object.
    pub fn req_obj<'a>(
        obj: &'a BTreeMap<String, JsonValue>,
        key: &str,
    ) -> Result<&'a BTreeMap<String, JsonValue>, JsonError> {
        match obj.get(key) {
            Some(Self::Object(m)) => Ok(m),
            Some(_) => Err(JsonError::Schema(format!(
                "field '{key}' must be an object"
            ))),
            None => Err(JsonError::Schema(format!(
                "missing required object field '{key}'"
            ))),
        }
    }

    /// Returns an optional object field from an object.
    pub fn opt_obj<'a>(
        obj: &'a BTreeMap<String, JsonValue>,
        key: &str,
    ) -> Result<Option<&'a BTreeMap<String, JsonValue>>, JsonError> {
        match obj.get(key) {
            None | Some(Self::Null) => Ok(None),
            Some(Self::Object(m)) => Ok(Some(m)),
            Some(_) => Err(JsonError::Schema(format!(
                "field '{key}' must be an object or null"
            ))),
        }
    }
}

fn write_escaped_str(s: &str, out: &mut String) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let code = c as u32;
                const HEX: &[u8; 16] = b"0123456789abcdef";
                out.push_str("\\u00");
                out.push(HEX[((code >> 4) & 0x0f) as usize] as char);
                out.push(HEX[(code & 0x0f) as usize] as char);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    budget: JsonBudget,
}

impl<'a> Parser<'a> {
    fn skip_ws(&mut self) {
        while let Some(&b) = self.bytes.get(self.pos) {
            if matches!(b, b' ' | b'\t' | b'\r' | b'\n') {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn parse_value(&mut self, depth: usize) -> Result<JsonValue, JsonError> {
        if depth > self.budget.max_depth {
            return Err(JsonError::MaxDepthExceeded {
                max: self.budget.max_depth,
            });
        }
        self.skip_ws();
        let b = self.peek().ok_or(JsonError::Syntax {
            offset: self.pos,
            reason: "unexpected end of JSON input",
        })?;
        match b {
            b'n' => self.parse_literal(b"null", JsonValue::Null),
            b't' => self.parse_literal(b"true", JsonValue::Bool(true)),
            b'f' => self.parse_literal(b"false", JsonValue::Bool(false)),
            b'"' => self.parse_string().map(JsonValue::Str),
            b'[' => self.parse_array(depth + 1),
            b'{' => self.parse_object(depth + 1),
            b'-' | b'0'..=b'9' => self.parse_number(),
            _ => Err(JsonError::Syntax {
                offset: self.pos,
                reason: "unexpected token",
            }),
        }
    }

    fn parse_literal(&mut self, expected: &[u8], val: JsonValue) -> Result<JsonValue, JsonError> {
        let end = self.pos.saturating_add(expected.len());
        if self.bytes.get(self.pos..end) == Some(expected) {
            self.pos = end;
            Ok(val)
        } else {
            Err(JsonError::Syntax {
                offset: self.pos,
                reason: "invalid literal token",
            })
        }
    }

    fn parse_string(&mut self) -> Result<String, JsonError> {
        if self.peek() != Some(b'"') {
            return Err(JsonError::Syntax {
                offset: self.pos,
                reason: "expected opening quote",
            });
        }
        self.pos += 1;
        let mut out = String::new();
        while let Some(b) = self.peek() {
            self.pos += 1;
            match b {
                b'"' => return Ok(out),
                b'\\' => {
                    let esc = self.peek().ok_or(JsonError::Syntax {
                        offset: self.pos,
                        reason: "unterminated escape sequence",
                    })?;
                    self.pos += 1;
                    let ch = match esc {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{0008}',
                        b'f' => '\u{000c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let hex_end = self.pos.saturating_add(4);
                            let slice =
                                self.bytes.get(self.pos..hex_end).ok_or(JsonError::Syntax {
                                    offset: self.pos,
                                    reason: "incomplete \\uXXXX escape",
                                })?;
                            let hex_str =
                                std::str::from_utf8(slice).map_err(|_| JsonError::Syntax {
                                    offset: self.pos,
                                    reason: "non-ASCII \\uXXXX escape",
                                })?;
                            let code = u32::from_str_radix(hex_str, 16).map_err(|_| {
                                JsonError::Syntax {
                                    offset: self.pos,
                                    reason: "invalid hex in \\uXXXX escape",
                                }
                            })?;
                            self.pos = hex_end;
                            char::from_u32(code).ok_or(JsonError::Syntax {
                                offset: self.pos,
                                reason: "invalid Unicode scalar value",
                            })?
                        }
                        _ => {
                            return Err(JsonError::Syntax {
                                offset: self.pos - 1,
                                reason: "unsupported escape character",
                            });
                        }
                    };
                    if out.len().saturating_add(ch.len_utf8()) > self.budget.max_string_bytes {
                        return Err(JsonError::StringTooLong {
                            max: self.budget.max_string_bytes,
                        });
                    }
                    out.push(ch);
                }
                c if c < 0x20 => {
                    return Err(JsonError::Syntax {
                        offset: self.pos - 1,
                        reason: "unescaped control character in string",
                    });
                }
                _ => {
                    let start = self.pos - 1;
                    while let Some(next_b) = self.peek() {
                        if next_b == b'"' || next_b == b'\\' || next_b < 0x20 {
                            break;
                        }
                        self.pos += 1;
                    }
                    let slice = self.bytes.get(start..self.pos).ok_or(JsonError::Syntax {
                        offset: start,
                        reason: "string slice out of bounds",
                    })?;
                    if out.len().saturating_add(slice.len()) > self.budget.max_string_bytes {
                        return Err(JsonError::StringTooLong {
                            max: self.budget.max_string_bytes,
                        });
                    }
                    let text = std::str::from_utf8(slice).map_err(|_| JsonError::Syntax {
                        offset: start,
                        reason: "invalid UTF-8 in JSON string",
                    })?;
                    out.push_str(text);
                }
            }
        }
        Err(JsonError::Syntax {
            offset: self.pos,
            reason: "unterminated string literal",
        })
    }

    fn parse_number(&mut self) -> Result<JsonValue, JsonError> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        let mut is_float = false;
        while let Some(b) = self.peek() {
            match b {
                b'0'..=b'9' => self.pos += 1,
                b'.' | b'e' | b'E' | b'+' | b'-' => {
                    is_float = true;
                    self.pos += 1;
                }
                _ => break,
            }
        }
        let slice = self.bytes.get(start..self.pos).ok_or(JsonError::Syntax {
            offset: start,
            reason: "empty number token",
        })?;
        let text = std::str::from_utf8(slice).map_err(|_| JsonError::Syntax {
            offset: start,
            reason: "non-ASCII number token",
        })?;
        if !is_float {
            let n = text.parse::<i64>().map_err(|_| JsonError::Syntax {
                offset: start,
                reason: "invalid 64-bit integer literal",
            })?;
            Ok(JsonValue::Int(n))
        } else {
            let v = text.parse::<f64>().map_err(|_| JsonError::Syntax {
                offset: start,
                reason: "invalid floating-point literal",
            })?;
            JsonValue::finite_float(v)
        }
    }

    fn parse_array(&mut self, depth: usize) -> Result<JsonValue, JsonError> {
        if depth > self.budget.max_depth {
            return Err(JsonError::MaxDepthExceeded {
                max: self.budget.max_depth,
            });
        }
        self.pos += 1; // '['
        self.skip_ws();
        let mut items = Vec::new();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(JsonValue::Array(items));
        }
        loop {
            if items.len() >= self.budget.max_container_items {
                return Err(JsonError::TooManyContainerItems {
                    max: self.budget.max_container_items,
                });
            }
            items.push(self.parse_value(depth)?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b']') => {
                    self.pos += 1;
                    return Ok(JsonValue::Array(items));
                }
                _ => {
                    return Err(JsonError::Syntax {
                        offset: self.pos,
                        reason: "expected ',' or ']' in array",
                    });
                }
            }
        }
    }

    fn parse_object(&mut self, depth: usize) -> Result<JsonValue, JsonError> {
        if depth > self.budget.max_depth {
            return Err(JsonError::MaxDepthExceeded {
                max: self.budget.max_depth,
            });
        }
        self.pos += 1; // '{'
        self.skip_ws();
        let mut map = BTreeMap::new();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(JsonValue::Object(map));
        }
        loop {
            if map.len() >= self.budget.max_container_items {
                return Err(JsonError::TooManyContainerItems {
                    max: self.budget.max_container_items,
                });
            }
            self.skip_ws();
            let key = self.parse_string()?;
            self.skip_ws();
            if self.peek() != Some(b':') {
                return Err(JsonError::Syntax {
                    offset: self.pos,
                    reason: "expected ':' after object key",
                });
            }
            self.pos += 1;
            let val = self.parse_value(depth)?;
            if map.contains_key(&key) {
                return Err(JsonError::DuplicateObjectKey(key));
            }
            map.insert(key, val);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(JsonValue::Object(map));
                }
                _ => {
                    return Err(JsonError::Syntax {
                        offset: self.pos,
                        reason: "expected ',' or '}' in object",
                    });
                }
            }
        }
    }
}
