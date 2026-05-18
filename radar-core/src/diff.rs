use indexmap::IndexMap;
use openapiv3::{
    OpenAPI, Parameter, PathItem, ReferenceOr, Schema, SchemaKind, StatusCode, Type,
};

use crate::{
    error::DriftError,
    models::{ChangeKind, Severity},
};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A change detected between two OpenAPI specs, before being persisted.
/// Unlike `Change` in models.rs, this has no DB identifiers.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DiffChange {
    /// Human-readable path, e.g. "GET /users/{id}" or
    /// "GET /users/{id} → param.filter" or
    /// "GET /users/{id} → response.phone"
    pub path: String,
    pub kind: ChangeKind,
    pub severity: Severity,
    pub description: Option<String>,
}

// ---------------------------------------------------------------------------
// parse_openapi
// ---------------------------------------------------------------------------

/// Parse an OpenAPI 3.x YAML string into an `openapiv3::OpenAPI` value.
pub fn parse_openapi(content: &str) -> Result<OpenAPI, DriftError> {
    serde_yaml::from_str(content).map_err(|e| DriftError::Parse(e.to_string()))
}

// ---------------------------------------------------------------------------
// diff_openapi
// ---------------------------------------------------------------------------

/// Compute the diff between two parsed OpenAPI specs and return the list of
/// detected changes.
pub fn diff_openapi(base: &OpenAPI, head: &OpenAPI) -> Vec<DiffChange> {
    let mut changes = Vec::new();

    let base_ops = collect_operations(base);
    let head_ops = collect_operations(head);

    // --- Operations removed or changed --------------------------------------
    for (key, base_op) in &base_ops {
        if !head_ops.contains_key(key) {
            changes.push(DiffChange {
                path: key.clone(),
                kind: ChangeKind::OperationRemoved,
                severity: Severity::Breaking,
                description: Some(format!("Operation {} was removed", key)),
            });
        } else {
            let head_op = &head_ops[key];
            diff_operation(key, base_op, head_op, &mut changes);
        }
    }

    // --- Operations added ---------------------------------------------------
    for (key, _head_op) in &head_ops {
        if !base_ops.contains_key(key) {
            changes.push(DiffChange {
                path: key.clone(),
                kind: ChangeKind::OperationAdded,
                severity: Severity::Safe,
                description: Some(format!("Operation {} was added", key)),
            });
        }
    }

    changes
}

// ---------------------------------------------------------------------------
// Internal helpers — operation collection
// ---------------------------------------------------------------------------

/// Collect all (method + path) → Operation pairs from a spec, keyed by the
/// human-readable string "METHOD /path".
fn collect_operations(spec: &OpenAPI) -> IndexMap<String, openapiv3::Operation> {
    let mut map = IndexMap::new();

    for (path_str, path_ref) in spec.paths.paths.iter() {
        let path_item: &PathItem = match path_ref {
            ReferenceOr::Item(pi) => pi,
            ReferenceOr::Reference { reference } => {
                eprintln!(
                    "warn: skipping $ref path item '{}' (full $ref resolution deferred)",
                    reference
                );
                continue;
            }
        };

        for (method, op) in path_item.iter() {
            let key = format!("{} {}", method.to_uppercase(), path_str);
            map.insert(key, op.clone());
        }
    }

    map
}

// ---------------------------------------------------------------------------
// Operation-level diffing
// ---------------------------------------------------------------------------

fn diff_operation(
    op_path: &str,
    base_op: &openapiv3::Operation,
    head_op: &openapiv3::Operation,
    changes: &mut Vec<DiffChange>,
) {
    diff_parameters(op_path, base_op, head_op, changes);
    diff_responses(op_path, base_op, head_op, changes);
}

// ---------------------------------------------------------------------------
// Parameter diffing
// ---------------------------------------------------------------------------

/// A key that uniquely identifies a parameter within an operation.
#[derive(Debug, PartialEq, Eq, Hash)]
struct ParamKey {
    name: String,
    location: String,
}

fn param_key(p: &Parameter) -> ParamKey {
    let (name, location) = match p {
        Parameter::Query { parameter_data, .. } => {
            (parameter_data.name.clone(), "query".to_string())
        }
        Parameter::Path { parameter_data, .. } => {
            (parameter_data.name.clone(), "path".to_string())
        }
        Parameter::Header { parameter_data, .. } => {
            (parameter_data.name.clone(), "header".to_string())
        }
        Parameter::Cookie { parameter_data, .. } => {
            (parameter_data.name.clone(), "cookie".to_string())
        }
    };
    ParamKey { name, location }
}

fn param_required(p: &Parameter) -> bool {
    p.parameter_data_ref().required
}

/// Collect resolved parameters from an operation, skipping $refs.
fn resolved_params(op: &openapiv3::Operation) -> IndexMap<ParamKey, Parameter> {
    let mut map = IndexMap::new();
    for param_ref in &op.parameters {
        match param_ref {
            ReferenceOr::Item(p) => {
                map.insert(param_key(p), p.clone());
            }
            ReferenceOr::Reference { reference } => {
                eprintln!(
                    "warn: skipping $ref parameter '{}' (full $ref resolution deferred)",
                    reference
                );
            }
        }
    }
    map
}

fn diff_parameters(
    op_path: &str,
    base_op: &openapiv3::Operation,
    head_op: &openapiv3::Operation,
    changes: &mut Vec<DiffChange>,
) {
    let base_params = resolved_params(base_op);
    let head_params = resolved_params(head_op);

    // Required param added in head → Breaking RequiredChanged
    for (key, head_p) in &head_params {
        if !base_params.contains_key(key) && param_required(head_p) {
            changes.push(DiffChange {
                path: format!("{} \u{2192} param.{}", op_path, key.name),
                kind: ChangeKind::RequiredChanged,
                severity: Severity::Breaking,
                description: Some(format!(
                    "Required {} parameter '{}' was added",
                    key.location, key.name
                )),
            });
        }
    }

    // Param type changed → Breaking TypeChanged
    for (key, base_p) in &base_params {
        if let Some(head_p) = head_params.get(key) {
            if let (Some(base_type), Some(head_type)) =
                (param_type_label(base_p), param_type_label(head_p))
            {
                if base_type != head_type {
                    changes.push(DiffChange {
                        path: format!("{} \u{2192} param.{}", op_path, key.name),
                        kind: ChangeKind::TypeChanged,
                        severity: Severity::Breaking,
                        description: Some(format!(
                            "Parameter '{}' type changed from '{}' to '{}'",
                            key.name, base_type, head_type
                        )),
                    });
                }
            }
        }
    }
}

/// Extract a simple type label string from a parameter's schema, if possible.
fn param_type_label(p: &Parameter) -> Option<String> {
    let schema_ref = match p.parameter_data_ref().format.clone() {
        openapiv3::ParameterSchemaOrContent::Schema(s) => s,
        openapiv3::ParameterSchemaOrContent::Content(_) => return None,
    };
    let schema = match schema_ref {
        ReferenceOr::Item(s) => s,
        ReferenceOr::Reference { .. } => return None,
    };
    type_label_from_kind(&schema.schema_kind)
}

// ---------------------------------------------------------------------------
// Response schema diffing
// ---------------------------------------------------------------------------

fn diff_responses(
    op_path: &str,
    base_op: &openapiv3::Operation,
    head_op: &openapiv3::Operation,
    changes: &mut Vec<DiffChange>,
) {
    let base_responses = &base_op.responses.responses;
    let head_responses = &head_op.responses.responses;

    for (status, base_resp_ref) in base_responses {
        if !is_2xx(status) {
            continue;
        }

        let base_resp = match base_resp_ref {
            ReferenceOr::Item(r) => r,
            ReferenceOr::Reference { reference } => {
                eprintln!(
                    "warn: skipping $ref response '{}' (full $ref resolution deferred)",
                    reference
                );
                continue;
            }
        };

        let head_resp = match head_responses.get(status) {
            Some(ReferenceOr::Item(r)) => r,
            Some(ReferenceOr::Reference { reference }) => {
                eprintln!(
                    "warn: skipping $ref response '{}' (full $ref resolution deferred)",
                    reference
                );
                continue;
            }
            None => continue,
        };

        // Compare application/json content schemas
        if let (Some(base_media), Some(head_media)) = (
            base_resp.content.get("application/json"),
            head_resp.content.get("application/json"),
        ) {
            if let (Some(base_schema_ref), Some(head_schema_ref)) =
                (&base_media.schema, &head_media.schema)
            {
                let base_schema = match base_schema_ref {
                    ReferenceOr::Item(s) => s,
                    ReferenceOr::Reference { reference } => {
                        eprintln!(
                            "warn: skipping $ref schema '{}' (full $ref resolution deferred)",
                            reference
                        );
                        continue;
                    }
                };
                let head_schema = match head_schema_ref {
                    ReferenceOr::Item(s) => s,
                    ReferenceOr::Reference { reference } => {
                        eprintln!(
                            "warn: skipping $ref schema '{}' (full $ref resolution deferred)",
                            reference
                        );
                        continue;
                    }
                };

                diff_schema_properties(op_path, "response", base_schema, head_schema, changes);
            }
        }
    }
}

fn is_2xx(status: &StatusCode) -> bool {
    match status {
        StatusCode::Code(n) => *n >= 200 && *n < 300,
        StatusCode::Range(n) => *n == 2,
    }
}

// ---------------------------------------------------------------------------
// Schema property diffing (recursive)
// ---------------------------------------------------------------------------

/// Recursively compare the properties of two object schemas.
/// `prefix` is the dot-separated label used after the arrow in DiffChange.path,
/// e.g. "response" or "response.user".
fn diff_schema_properties(
    op_path: &str,
    prefix: &str,
    base_schema: &Schema,
    head_schema: &Schema,
    changes: &mut Vec<DiffChange>,
) {
    let (base_obj, head_obj) = match (&base_schema.schema_kind, &head_schema.schema_kind) {
        (SchemaKind::Type(Type::Object(b)), SchemaKind::Type(Type::Object(h))) => (b, h),
        _ => return, // Only handle pure Object types for now
    };

    // Properties removed → FieldRemoved (Breaking)
    for (prop_name, _) in &base_obj.properties {
        if !head_obj.properties.contains_key(prop_name) {
            changes.push(DiffChange {
                path: format!("{} \u{2192} {}.{}", op_path, prefix, prop_name),
                kind: ChangeKind::FieldRemoved,
                severity: Severity::Breaking,
                description: Some(format!("Response property '{}' was removed", prop_name)),
            });
        }
    }

    // Properties added → FieldAdded (Safe)
    for (prop_name, _) in &head_obj.properties {
        if !base_obj.properties.contains_key(prop_name) {
            changes.push(DiffChange {
                path: format!("{} \u{2192} {}.{}", op_path, prefix, prop_name),
                kind: ChangeKind::FieldAdded,
                severity: Severity::Safe,
                description: Some(format!("Response property '{}' was added", prop_name)),
            });
        }
    }

    // Properties present in both: compare type and requiredness
    for (prop_name, base_prop_ref) in &base_obj.properties {
        let head_prop_ref = match head_obj.properties.get(prop_name) {
            Some(r) => r,
            None => continue, // already emitted FieldRemoved
        };

        let base_prop_schema: &Schema = match base_prop_ref {
            ReferenceOr::Item(s) => s,
            ReferenceOr::Reference { reference } => {
                eprintln!(
                    "warn: skipping $ref property schema '{}' (full $ref resolution deferred)",
                    reference
                );
                continue;
            }
        };
        let head_prop_schema: &Schema = match head_prop_ref {
            ReferenceOr::Item(s) => s,
            ReferenceOr::Reference { reference } => {
                eprintln!(
                    "warn: skipping $ref property schema '{}' (full $ref resolution deferred)",
                    reference
                );
                continue;
            }
        };

        // Type changed? → Breaking TypeChanged
        if let (Some(base_type), Some(head_type)) = (
            type_label_from_kind(&base_prop_schema.schema_kind),
            type_label_from_kind(&head_prop_schema.schema_kind),
        ) {
            if base_type != head_type {
                changes.push(DiffChange {
                    path: format!("{} \u{2192} {}.{}", op_path, prefix, prop_name),
                    kind: ChangeKind::TypeChanged,
                    severity: Severity::Breaking,
                    description: Some(format!(
                        "Property '{}' type changed from '{}' to '{}'",
                        prop_name, base_type, head_type
                    )),
                });
            }
        }

        // Required status changed?
        let base_required = base_obj.required.contains(prop_name);
        let head_required = head_obj.required.contains(prop_name);

        if base_required && !head_required {
            // required → optional: NonBreakingRisky
            changes.push(DiffChange {
                path: format!("{} \u{2192} {}.{}", op_path, prefix, prop_name),
                kind: ChangeKind::RequiredChanged,
                severity: Severity::NonBreakingRisky,
                description: Some(format!(
                    "Property '{}' changed from required to optional",
                    prop_name
                )),
            });
        } else if !base_required && head_required {
            // optional → required: Safe
            changes.push(DiffChange {
                path: format!("{} \u{2192} {}.{}", op_path, prefix, prop_name),
                kind: ChangeKind::RequiredChanged,
                severity: Severity::Safe,
                description: Some(format!(
                    "Property '{}' changed from optional to required",
                    prop_name
                )),
            });
        }

        // Recurse into nested objects
        let nested_prefix = format!("{}.{}", prefix, prop_name);
        diff_schema_properties(
            op_path,
            &nested_prefix,
            base_prop_schema,
            head_prop_schema,
            changes,
        );
    }
}

/// Return a short string describing the primitive type of a schema kind.
/// Returns `None` for complex/compound kinds.
fn type_label_from_kind(kind: &SchemaKind) -> Option<String> {
    match kind {
        SchemaKind::Type(t) => Some(
            match t {
                Type::String(_) => "string",
                Type::Number(_) => "number",
                Type::Integer(_) => "integer",
                Type::Boolean(_) => "boolean",
                Type::Array(_) => "array",
                Type::Object(_) => "object",
            }
            .to_string(),
        ),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tests (TDD — written before the implementation above)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ChangeKind, Severity};

    /// Parse a minimal OpenAPI YAML string, panicking on failure.
    fn parse(yaml: &str) -> OpenAPI {
        parse_openapi(yaml).expect("parse failed")
    }

    // -----------------------------------------------------------------------
    // 1. Identical specs → no changes
    // -----------------------------------------------------------------------
    #[test]
    fn test_identical_specs_no_changes() {
        let spec_yaml = r#"
openapi: "3.0.0"
info:
  title: Test
  version: "1"
paths:
  /users:
    get:
      responses:
        '200':
          description: ok
"#;
        let spec = parse(spec_yaml);
        let changes = diff_openapi(&spec, &spec);
        assert!(
            changes.is_empty(),
            "Expected no changes for identical specs, got: {:?}",
            changes
        );
    }

    // -----------------------------------------------------------------------
    // 2. Operation removed → Breaking OperationRemoved
    // -----------------------------------------------------------------------
    #[test]
    fn test_operation_removed_is_breaking() {
        let base_yaml = r#"
openapi: "3.0.0"
info:
  title: Test
  version: "1"
paths:
  /users:
    get:
      responses:
        '200':
          description: ok
"#;
        let head_yaml = r#"
openapi: "3.0.0"
info:
  title: Test
  version: "1"
paths: {}
"#;
        let base = parse(base_yaml);
        let head = parse(head_yaml);
        let changes = diff_openapi(&base, &head);

        assert_eq!(changes.len(), 1, "Expected exactly 1 change, got: {:?}", changes);
        let c = &changes[0];
        assert_eq!(c.kind, ChangeKind::OperationRemoved);
        assert_eq!(c.severity, Severity::Breaking);
        assert_eq!(c.path, "GET /users");
    }

    // -----------------------------------------------------------------------
    // 3. Operation added → Safe OperationAdded
    // -----------------------------------------------------------------------
    #[test]
    fn test_operation_added_is_safe() {
        let base_yaml = r#"
openapi: "3.0.0"
info:
  title: Test
  version: "1"
paths: {}
"#;
        let head_yaml = r#"
openapi: "3.0.0"
info:
  title: Test
  version: "1"
paths:
  /admin:
    get:
      responses:
        '200':
          description: ok
"#;
        let base = parse(base_yaml);
        let head = parse(head_yaml);
        let changes = diff_openapi(&base, &head);

        assert_eq!(changes.len(), 1, "Expected exactly 1 change, got: {:?}", changes);
        let c = &changes[0];
        assert_eq!(c.kind, ChangeKind::OperationAdded);
        assert_eq!(c.severity, Severity::Safe);
        assert_eq!(c.path, "GET /admin");
    }

    // -----------------------------------------------------------------------
    // 4. Response field removed → Breaking FieldRemoved
    // -----------------------------------------------------------------------
    #[test]
    fn test_response_field_removed_is_breaking() {
        let base_yaml = r#"
openapi: "3.0.0"
info:
  title: Test
  version: "1"
paths:
  /users:
    get:
      responses:
        '200':
          description: ok
          content:
            application/json:
              schema:
                type: object
                properties:
                  id:
                    type: string
                  phone:
                    type: string
"#;
        let head_yaml = r#"
openapi: "3.0.0"
info:
  title: Test
  version: "1"
paths:
  /users:
    get:
      responses:
        '200':
          description: ok
          content:
            application/json:
              schema:
                type: object
                properties:
                  id:
                    type: string
"#;
        let base = parse(base_yaml);
        let head = parse(head_yaml);
        let changes = diff_openapi(&base, &head);

        let field_removed: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::FieldRemoved && c.severity == Severity::Breaking)
            .collect();
        assert_eq!(
            field_removed.len(),
            1,
            "Expected exactly 1 FieldRemoved/Breaking change, got: {:?}",
            changes
        );
        assert!(
            field_removed[0].path.contains("phone"),
            "Expected path to mention 'phone', got: {}",
            field_removed[0].path
        );
    }

    // -----------------------------------------------------------------------
    // 5. Response field added → Safe FieldAdded
    // -----------------------------------------------------------------------
    #[test]
    fn test_response_field_added_is_safe() {
        let base_yaml = r#"
openapi: "3.0.0"
info:
  title: Test
  version: "1"
paths:
  /users:
    get:
      responses:
        '200':
          description: ok
          content:
            application/json:
              schema:
                type: object
                properties:
                  id:
                    type: string
"#;
        let head_yaml = r#"
openapi: "3.0.0"
info:
  title: Test
  version: "1"
paths:
  /users:
    get:
      responses:
        '200':
          description: ok
          content:
            application/json:
              schema:
                type: object
                properties:
                  id:
                    type: string
                  email:
                    type: string
"#;
        let base = parse(base_yaml);
        let head = parse(head_yaml);
        let changes = diff_openapi(&base, &head);

        let field_added: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::FieldAdded && c.severity == Severity::Safe)
            .collect();
        assert_eq!(
            field_added.len(),
            1,
            "Expected exactly 1 FieldAdded/Safe change, got: {:?}",
            changes
        );
        assert!(
            field_added[0].path.contains("email"),
            "Expected path to mention 'email', got: {}",
            field_added[0].path
        );
    }

    // -----------------------------------------------------------------------
    // 6. Required query param added in head → Breaking RequiredChanged
    // -----------------------------------------------------------------------
    #[test]
    fn test_required_param_added_is_breaking() {
        let base_yaml = r#"
openapi: "3.0.0"
info:
  title: Test
  version: "1"
paths:
  /users:
    get:
      responses:
        '200':
          description: ok
"#;
        let head_yaml = r#"
openapi: "3.0.0"
info:
  title: Test
  version: "1"
paths:
  /users:
    get:
      parameters:
        - name: filter
          in: query
          required: true
          schema:
            type: string
      responses:
        '200':
          description: ok
"#;
        let base = parse(base_yaml);
        let head = parse(head_yaml);
        let changes = diff_openapi(&base, &head);

        let required_changed: Vec<_> = changes
            .iter()
            .filter(|c| {
                c.kind == ChangeKind::RequiredChanged && c.severity == Severity::Breaking
            })
            .collect();
        assert_eq!(
            required_changed.len(),
            1,
            "Expected exactly 1 RequiredChanged/Breaking change, got: {:?}",
            changes
        );
        assert!(
            required_changed[0].path.contains("filter"),
            "Expected path to mention 'filter', got: {}",
            required_changed[0].path
        );
    }

    // -----------------------------------------------------------------------
    // 7. Property type changed → Breaking TypeChanged
    // -----------------------------------------------------------------------
    #[test]
    fn test_property_type_changed_is_breaking() {
        let base_yaml = r#"
openapi: "3.0.0"
info:
  title: Test
  version: "1"
paths:
  /users:
    get:
      responses:
        '200':
          description: ok
          content:
            application/json:
              schema:
                type: object
                properties:
                  id:
                    type: string
"#;
        let head_yaml = r#"
openapi: "3.0.0"
info:
  title: Test
  version: "1"
paths:
  /users:
    get:
      responses:
        '200':
          description: ok
          content:
            application/json:
              schema:
                type: object
                properties:
                  id:
                    type: integer
"#;
        let base = parse(base_yaml);
        let head = parse(head_yaml);
        let changes = diff_openapi(&base, &head);

        let type_changed: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::TypeChanged && c.severity == Severity::Breaking)
            .collect();
        assert_eq!(
            type_changed.len(),
            1,
            "Expected exactly 1 TypeChanged/Breaking change, got: {:?}",
            changes
        );
        assert!(
            type_changed[0].path.contains("id"),
            "Expected path to mention 'id', got: {}",
            type_changed[0].path
        );
    }
}
