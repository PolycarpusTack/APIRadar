use std::collections::HashMap;

use crate::{
    diff::DiffChange,
    error::DriftError,
    models::{ChangeKind, Severity},
};

// ---------------------------------------------------------------------------
// Intermediate representation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct ProtoField {
    pub name: String,
    pub type_name: String,
    pub number: u32,
}

#[derive(Debug, Clone)]
pub struct ProtoMessage {
    pub name: String,
    pub fields: Vec<ProtoField>,
}

#[derive(Debug, Clone)]
pub struct ProtoEnum {
    pub name: String,
    pub values: Vec<(String, i32)>,
}

#[derive(Debug, Clone, Default)]
pub struct ProtoSchema {
    pub messages: HashMap<String, ProtoMessage>,
    pub enums: HashMap<String, ProtoEnum>,
}

// ---------------------------------------------------------------------------
// parse_proto — minimal proto3 parser (no external binary required)
// ---------------------------------------------------------------------------

pub fn parse_proto(content: &str) -> Result<ProtoSchema, DriftError> {
    let stripped = strip_comments(content);
    let mut schema = ProtoSchema::default();
    parse_body(stripped.as_bytes(), &mut schema);

    // Reject input that does not look like protobuf at all. Without this a wrong
    // format (e.g. an OpenAPI YAML passed with --format proto) or a corrupted
    // file parses to an empty schema and is silently reported as "no changes".
    // A valid-but-empty proto (`syntax = "proto3";` with no messages) is still
    // accepted because it carries a syntax declaration.
    if schema.messages.is_empty() && schema.enums.is_empty() && !has_proto_syntax(&stripped) {
        return Err(DriftError::Parse(
            "input does not appear to be a protobuf schema \
             (no syntax declaration, message, or enum found)"
                .to_string(),
        ));
    }

    Ok(schema)
}

/// True when the (comment-stripped) input contains a `syntax = "proto…"` line.
fn has_proto_syntax(stripped: &str) -> bool {
    stripped.lines().any(|line| {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("syntax") {
            let rest = rest.trim_start();
            if let Some(after_eq) = rest.strip_prefix('=') {
                return after_eq.trim_start().starts_with("\"proto");
            }
        }
        false
    })
}

// ---------------------------------------------------------------------------
// Parser internals
// ---------------------------------------------------------------------------

fn strip_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    loop {
        match chars.next() {
            None => break,
            Some('/') => match chars.peek().copied() {
                Some('/') => {
                    for c in chars.by_ref() {
                        if c == '\n' {
                            out.push('\n');
                            break;
                        }
                    }
                }
                Some('*') => {
                    chars.next();
                    loop {
                        match chars.next() {
                            None => break,
                            Some('*') if chars.peek() == Some(&'/') => {
                                chars.next();
                                break;
                            }
                            Some('\n') => out.push('\n'),
                            _ => {}
                        }
                    }
                }
                _ => out.push('/'),
            },
            Some(c) => out.push(c),
        }
    }
    out
}

/// Scan bytes and extract top-level message/enum blocks.
fn parse_body(bytes: &[u8], schema: &mut ProtoSchema) {
    let mut pos = 0;

    while pos < bytes.len() {
        skip_ws(bytes, &mut pos);
        if pos >= bytes.len() {
            break;
        }

        if word_at(bytes, pos, b"message") {
            pos += 7;
            skip_ws(bytes, &mut pos);
            let name = read_ident(bytes, &mut pos);
            skip_ws(bytes, &mut pos);
            if pos < bytes.len() && bytes[pos] == b'{' {
                pos += 1;
                let (fields, end) = read_message_body(bytes, pos);
                if !name.is_empty() {
                    schema
                        .messages
                        .insert(name.clone(), ProtoMessage { name, fields });
                }
                pos = end;
            }
        } else if word_at(bytes, pos, b"enum") {
            pos += 4;
            skip_ws(bytes, &mut pos);
            let name = read_ident(bytes, &mut pos);
            skip_ws(bytes, &mut pos);
            if pos < bytes.len() && bytes[pos] == b'{' {
                pos += 1;
                let (values, end) = read_enum_body(bytes, pos);
                if !name.is_empty() {
                    schema
                        .enums
                        .insert(name.clone(), ProtoEnum { name, values });
                }
                pos = end;
            }
        } else {
            // Skip to end of statement (';') or end of line, whichever comes first.
            while pos < bytes.len() && bytes[pos] != b';' && bytes[pos] != b'\n' {
                pos += 1;
            }
            if pos < bytes.len() {
                pos += 1; // consume ';' or '\n'
            }
        }
    }
}

fn skip_ws(bytes: &[u8], pos: &mut usize) {
    while *pos < bytes.len() && bytes[*pos].is_ascii_whitespace() {
        *pos += 1;
    }
}

fn read_ident(bytes: &[u8], pos: &mut usize) -> String {
    let start = *pos;
    while *pos < bytes.len()
        && (bytes[*pos].is_ascii_alphanumeric() || bytes[*pos] == b'_' || bytes[*pos] == b'.')
    {
        *pos += 1;
    }
    String::from_utf8_lossy(&bytes[start..*pos]).into_owned()
}

fn word_at(bytes: &[u8], pos: usize, word: &[u8]) -> bool {
    let end = pos + word.len();
    if end > bytes.len() {
        return false;
    }
    if bytes[pos..end] != *word {
        return false;
    }
    // Must be followed by whitespace or '{' — not another identifier char
    matches!(
        bytes.get(end).copied(),
        None | Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r') | Some(b'{')
    )
}

/// Read message body (after opening `{`), returning (fields, pos_after_closing_brace).
fn read_message_body(bytes: &[u8], start: usize) -> (Vec<ProtoField>, usize) {
    let mut fields = Vec::new();
    let mut pos = start;
    let mut depth = 1usize;

    while pos < bytes.len() && depth > 0 {
        skip_ws(bytes, &mut pos);
        if pos >= bytes.len() {
            break;
        }

        match bytes[pos] {
            b'{' => {
                depth += 1;
                pos += 1;
            }
            b'}' => {
                depth -= 1;
                pos += 1;
            }
            _ if depth == 1 => {
                // Read statement up to ';', '{', or '}'
                let stmt_start = pos;
                while pos < bytes.len()
                    && bytes[pos] != b';'
                    && bytes[pos] != b'{'
                    && bytes[pos] != b'}'
                {
                    pos += 1;
                }
                let stmt = String::from_utf8_lossy(&bytes[stmt_start..pos]).into_owned();

                // `oneof <name> { <fields> }` — its members share the message's
                // field-number space, so parse them as fields of this message
                // rather than skipping the nested block.
                if pos < bytes.len() && bytes[pos] == b'{' && stmt.trim_start().starts_with("oneof")
                {
                    pos += 1; // consume '{'
                    read_oneof_fields(bytes, &mut pos, &mut fields);
                    continue;
                }

                if pos < bytes.len() && bytes[pos] == b';' {
                    pos += 1; // consume ';'
                }
                if let Some(f) = parse_field_stmt(&stmt) {
                    fields.push(f);
                }
            }
            _ => {
                pos += 1;
            }
        }
    }

    (fields, pos)
}

/// Read the fields inside a `oneof { … }` block (pos is just after the `{`),
/// pushing each into `fields` and leaving pos just after the closing `}`.
fn read_oneof_fields(bytes: &[u8], pos: &mut usize, fields: &mut Vec<ProtoField>) {
    loop {
        skip_ws(bytes, pos);
        if *pos >= bytes.len() {
            break;
        }
        if bytes[*pos] == b'}' {
            *pos += 1; // consume closing '}'
            break;
        }
        let stmt_start = *pos;
        while *pos < bytes.len() && bytes[*pos] != b';' && bytes[*pos] != b'}' {
            *pos += 1;
        }
        let stmt = String::from_utf8_lossy(&bytes[stmt_start..*pos]).into_owned();
        if *pos < bytes.len() && bytes[*pos] == b';' {
            *pos += 1; // consume ';'
        }
        if let Some(f) = parse_field_stmt(&stmt) {
            fields.push(f);
        }
    }
}

/// Read enum body (after opening `{`), returning (values, pos_after_closing_brace).
fn read_enum_body(bytes: &[u8], start: usize) -> (Vec<(String, i32)>, usize) {
    let mut values = Vec::new();
    let mut pos = start;
    let mut depth = 1usize;

    while pos < bytes.len() && depth > 0 {
        skip_ws(bytes, &mut pos);
        if pos >= bytes.len() {
            break;
        }

        match bytes[pos] {
            b'{' => {
                depth += 1;
                pos += 1;
            }
            b'}' => {
                depth -= 1;
                pos += 1;
            }
            _ if depth == 1 => {
                let stmt_start = pos;
                while pos < bytes.len()
                    && bytes[pos] != b';'
                    && bytes[pos] != b'{'
                    && bytes[pos] != b'}'
                {
                    pos += 1;
                }
                let stmt = String::from_utf8_lossy(&bytes[stmt_start..pos]).into_owned();
                if pos < bytes.len() && bytes[pos] == b';' {
                    pos += 1;
                }
                if let Some(v) = parse_enum_value_stmt(&stmt) {
                    values.push(v);
                }
            }
            _ => {
                pos += 1;
            }
        }
    }

    (values, pos)
}

/// Parse a proto3 field statement like `string name = 1` (without trailing `;`).
/// Handles: `repeated type name = number`, `optional type name = number`, `type name = number`.
fn parse_field_stmt(stmt: &str) -> Option<ProtoField> {
    let trimmed = stmt.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Skip option/reserved/oneof/syntax/package lines
    let first_word = trimmed.split_ascii_whitespace().next().unwrap_or("");
    if matches!(
        first_word,
        "option" | "reserved" | "oneof" | "syntax" | "package" | "import" | "message" | "enum"
    ) {
        return None;
    }

    // `map<K, V> name = number` — the angle-bracket type may contain a space and
    // a comma, so it cannot be tokenised by whitespace. Treat the whole `map<…>`
    // as the type (whitespace-normalised so `map<string, string>` == `map<string,string>`).
    if let Some(rest) = trimmed.strip_prefix("map<") {
        let gt = rest.find('>')?;
        let inner: String = rest[..gt].split_whitespace().collect();
        let type_name = format!("map<{inner}>");
        let after = rest[gt + 1..].trim();
        let mut parts = after.split_ascii_whitespace();
        let field_name = parts.next()?.to_string();
        if parts.next()? != "=" {
            return None;
        }
        let num_raw = parts.next()?;
        let num_str = num_raw
            .split('[')
            .next()
            .unwrap_or(num_raw)
            .trim_end_matches(';');
        let number: u32 = num_str.parse().ok()?;
        return Some(ProtoField {
            name: field_name,
            type_name,
            number,
        });
    }

    let mut parts = trimmed.split_ascii_whitespace();
    let first = parts.next()?;

    let (type_name, field_name) = if matches!(first, "repeated" | "optional" | "required") {
        let ty = parts.next()?;
        let nm = parts.next()?;
        (
            if first == "repeated" {
                format!("repeated {ty}")
            } else {
                ty.to_string()
            },
            nm.to_string(),
        )
    } else {
        let nm = parts.next()?;
        (first.to_string(), nm.to_string())
    };

    // Expect "="
    if parts.next()? != "=" {
        return None;
    }

    // Field number (may be followed by `[...options...]`)
    let num_raw = parts.next()?;
    let num_str = num_raw
        .split('[')
        .next()
        .unwrap_or(num_raw)
        .trim_end_matches(';');
    let number: u32 = num_str.parse().ok()?;

    Some(ProtoField {
        name: field_name,
        type_name,
        number,
    })
}

/// Parse an enum value statement like `ACTIVE = 0`.
fn parse_enum_value_stmt(stmt: &str) -> Option<(String, i32)> {
    let trimmed = stmt.trim();
    if trimmed.is_empty() {
        return None;
    }
    let first_word = trimmed.split_ascii_whitespace().next().unwrap_or("");
    if matches!(first_word, "option" | "reserved") {
        return None;
    }

    let mut parts = trimmed.split_ascii_whitespace();
    let name = parts.next()?.to_string();
    if parts.next()? != "=" {
        return None;
    }
    let num_raw = parts.next()?;
    let num_str = num_raw
        .split('[')
        .next()
        .unwrap_or(num_raw)
        .trim_end_matches(';');
    let number: i32 = num_str.parse().ok()?;
    Some((name, number))
}

// ---------------------------------------------------------------------------
// diff_proto
// ---------------------------------------------------------------------------

pub fn diff_proto(base: &ProtoSchema, head: &ProtoSchema) -> Vec<DiffChange> {
    let mut changes = Vec::new();

    // Messages
    for (name, base_msg) in &base.messages {
        match head.messages.get(name) {
            None => changes.push(DiffChange {
                path: format!("message {name}"),
                kind: ChangeKind::OperationRemoved,
                severity: Severity::Breaking,
                description: Some(format!("Message '{name}' was removed")),
            }),
            Some(head_msg) => diff_message(base_msg, head_msg, &mut changes),
        }
    }
    for name in head.messages.keys() {
        if !base.messages.contains_key(name) {
            changes.push(DiffChange {
                path: format!("message {name}"),
                kind: ChangeKind::OperationAdded,
                severity: Severity::Safe,
                description: Some(format!("Message '{name}' was added")),
            });
        }
    }

    // Enums
    for (name, base_enum) in &base.enums {
        match head.enums.get(name) {
            None => changes.push(DiffChange {
                path: format!("enum {name}"),
                kind: ChangeKind::OperationRemoved,
                severity: Severity::Breaking,
                description: Some(format!("Enum '{name}' was removed")),
            }),
            Some(head_enum) => diff_enum(base_enum, head_enum, &mut changes),
        }
    }
    for name in head.enums.keys() {
        if !base.enums.contains_key(name) {
            changes.push(DiffChange {
                path: format!("enum {name}"),
                kind: ChangeKind::OperationAdded,
                severity: Severity::Safe,
                description: Some(format!("Enum '{name}' was added")),
            });
        }
    }

    changes
}

fn diff_message(base: &ProtoMessage, head: &ProtoMessage, changes: &mut Vec<DiffChange>) {
    // Proto wire format identity = field number. Diff by number.
    let base_by_num: HashMap<u32, &ProtoField> =
        base.fields.iter().map(|f| (f.number, f)).collect();
    let head_by_num: HashMap<u32, &ProtoField> =
        head.fields.iter().map(|f| (f.number, f)).collect();

    for (num, bf) in &base_by_num {
        match head_by_num.get(num) {
            None => changes.push(DiffChange {
                path: format!("{}.{}", base.name, bf.name),
                kind: ChangeKind::FieldRemoved,
                severity: Severity::Breaking,
                description: Some(format!(
                    "Field '{}.{}' (number {num}) was removed",
                    base.name, bf.name
                )),
            }),
            Some(hf) => {
                if bf.type_name != hf.type_name {
                    changes.push(DiffChange {
                        path: format!("{}.{}", base.name, bf.name),
                        kind: ChangeKind::TypeChanged,
                        severity: Severity::Breaking,
                        description: Some(format!(
                            "Field '{}.{}' type changed from '{}' to '{}'",
                            base.name, bf.name, bf.type_name, hf.type_name
                        )),
                    });
                }
                if bf.name != hf.name {
                    changes.push(DiffChange {
                        path: format!("{}.{}", base.name, bf.name),
                        kind: ChangeKind::FieldRemoved,
                        severity: Severity::NonBreakingRisky,
                        description: Some(format!(
                            "Field '{}.{}' (number {num}) renamed to '{}'",
                            base.name, bf.name, hf.name
                        )),
                    });
                }
            }
        }
    }

    for (num, hf) in &head_by_num {
        if !base_by_num.contains_key(num) {
            changes.push(DiffChange {
                path: format!("{}.{}", base.name, hf.name),
                kind: ChangeKind::FieldAdded,
                severity: Severity::Safe,
                description: Some(format!(
                    "Field '{}.{}' (number {num}) was added",
                    base.name, hf.name
                )),
            });
        }
    }
}

fn diff_enum(base: &ProtoEnum, head: &ProtoEnum, changes: &mut Vec<DiffChange>) {
    let base_names: std::collections::HashSet<&str> =
        base.values.iter().map(|(n, _)| n.as_str()).collect();
    let head_names: std::collections::HashSet<&str> =
        head.values.iter().map(|(n, _)| n.as_str()).collect();

    for name in base_names.difference(&head_names) {
        changes.push(DiffChange {
            path: format!("{}.{name}", base.name),
            kind: ChangeKind::FieldRemoved,
            severity: Severity::Breaking,
            description: Some(format!("Enum value '{}.{name}' was removed", base.name)),
        });
    }
    for name in head_names.difference(&base_names) {
        changes.push(DiffChange {
            path: format!("{}.{name}", base.name),
            kind: ChangeKind::FieldAdded,
            severity: Severity::Safe,
            description: Some(format!("Enum value '{}.{name}' was added", base.name)),
        });
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(proto: &str) -> ProtoSchema {
        parse_proto(proto).expect("parse failed")
    }

    #[test]
    fn test_identical_proto_no_changes() {
        let s = r#"
            syntax = "proto3";
            message User { string name = 1; int32 id = 2; }
        "#;
        let schema = parse(s);
        let changes = diff_proto(&schema, &schema);
        assert!(changes.is_empty(), "unexpected changes: {changes:?}");
    }

    #[test]
    fn test_message_removed_is_breaking() {
        let base = parse(r#"syntax="proto3"; message User { string name = 1; }"#);
        let head = parse(r#"syntax="proto3";"#);
        let changes = diff_proto(&base, &head);
        let removed: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::OperationRemoved && c.severity == Severity::Breaking)
            .collect();
        assert_eq!(removed.len(), 1);
        assert!(removed[0].path.contains("User"));
    }

    #[test]
    fn test_field_removed_is_breaking() {
        let base = parse(r#"syntax="proto3"; message User { string name = 1; string phone = 2; }"#);
        let head = parse(r#"syntax="proto3"; message User { string name = 1; }"#);
        let changes = diff_proto(&base, &head);
        let removed: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::FieldRemoved && c.severity == Severity::Breaking)
            .collect();
        assert_eq!(removed.len(), 1, "got: {changes:?}");
        assert!(removed[0].path.contains("phone"));
    }

    #[test]
    fn test_field_added_is_safe() {
        let base = parse(r#"syntax="proto3"; message User { string name = 1; }"#);
        let head = parse(r#"syntax="proto3"; message User { string name = 1; string email = 2; }"#);
        let changes = diff_proto(&base, &head);
        let added: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::FieldAdded && c.severity == Severity::Safe)
            .collect();
        assert_eq!(added.len(), 1, "got: {changes:?}");
        assert!(added[0].path.contains("email"));
    }

    #[test]
    fn test_field_type_changed_is_breaking() {
        let base = parse(r#"syntax="proto3"; message User { string id = 1; }"#);
        let head = parse(r#"syntax="proto3"; message User { int32 id = 1; }"#);
        let changes = diff_proto(&base, &head);
        let type_changed: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::TypeChanged && c.severity == Severity::Breaking)
            .collect();
        assert_eq!(type_changed.len(), 1, "got: {changes:?}");
    }

    #[test]
    fn test_enum_value_removed_is_breaking() {
        let base =
            parse(r#"syntax="proto3"; enum Status { ACTIVE = 0; INACTIVE = 1; PENDING = 2; }"#);
        let head = parse(r#"syntax="proto3"; enum Status { ACTIVE = 0; INACTIVE = 1; }"#);
        let changes = diff_proto(&base, &head);
        let removed: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::FieldRemoved && c.severity == Severity::Breaking)
            .collect();
        assert_eq!(removed.len(), 1, "got: {changes:?}");
        assert!(removed[0].path.contains("PENDING"));
    }

    #[test]
    fn test_repeated_field_added_is_safe() {
        let base = parse(r#"syntax="proto3"; message User { string name = 1; }"#);
        let head = parse(
            r#"syntax="proto3"; message User { string name = 1; repeated string tags = 2; }"#,
        );
        let changes = diff_proto(&base, &head);
        assert!(
            changes.iter().all(|c| c.severity == Severity::Safe),
            "expected all safe, got: {changes:?}"
        );
    }

    // -----------------------------------------------------------------------
    // M-5: non-proto / malformed input returns Err (not an empty schema)
    // -----------------------------------------------------------------------
    #[test]
    fn test_non_proto_input_is_err() {
        // An OpenAPI YAML accidentally passed as proto.
        assert!(parse_proto("openapi: \"3.0.0\"\ninfo:\n  title: X\n").is_err());
        // Random garbage.
        assert!(parse_proto("!!! not a schema at all ###").is_err());
        // Empty input.
        assert!(parse_proto("").is_err());
    }

    #[test]
    fn test_empty_but_valid_proto_is_ok() {
        // A syntax declaration with no messages is legitimately empty.
        assert!(parse_proto(r#"syntax = "proto3";"#).is_ok());
    }

    // -----------------------------------------------------------------------
    // M-5: oneof member removal is detected
    // -----------------------------------------------------------------------
    #[test]
    fn test_oneof_member_removed_is_breaking() {
        let base =
            parse(r#"syntax="proto3"; message M { oneof choice { string a = 1; int32 b = 2; } }"#);
        let head = parse(r#"syntax="proto3"; message M { oneof choice { string a = 1; } }"#);
        let changes = diff_proto(&base, &head);
        let removed: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::FieldRemoved && c.severity == Severity::Breaking)
            .collect();
        assert_eq!(
            removed.len(),
            1,
            "oneof member removal must be detected, got: {changes:?}"
        );
        assert!(removed[0].path.contains('b'));
    }

    // -----------------------------------------------------------------------
    // M-5: map<> field is parsed and its removal detected
    // -----------------------------------------------------------------------
    #[test]
    fn test_map_field_removed_is_breaking() {
        let base = parse(
            r#"syntax="proto3"; message M { string id = 1; map<string, string> labels = 2; }"#,
        );
        let head = parse(r#"syntax="proto3"; message M { string id = 1; }"#);
        let changes = diff_proto(&base, &head);
        let removed: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::FieldRemoved && c.severity == Severity::Breaking)
            .collect();
        assert_eq!(
            removed.len(),
            1,
            "map field removal must be detected, got: {changes:?}"
        );
        assert!(removed[0].path.contains("labels"));
    }

    #[test]
    fn test_map_field_value_type_change_is_breaking() {
        let base = parse(r#"syntax="proto3"; message M { map<string, string> labels = 1; }"#);
        let head = parse(r#"syntax="proto3"; message M { map<string, int32> labels = 1; }"#);
        let changes = diff_proto(&base, &head);
        let changed: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::TypeChanged && c.severity == Severity::Breaking)
            .collect();
        assert_eq!(
            changed.len(),
            1,
            "map value type change must be detected, got: {changes:?}"
        );
    }
}
