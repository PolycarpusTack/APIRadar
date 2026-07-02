use indexmap::IndexMap;
use openapiv3::{
    OpenAPI, Parameter, PathItem, ReferenceOr, RequestBody, Response, Schema, SchemaKind,
    StatusCode, Type,
};

use crate::{
    error::DriftError,
    models::{ChangeKind, Severity},
};

// ---------------------------------------------------------------------------
// $ref resolvers — only local component refs (#/components/…) are supported
// ---------------------------------------------------------------------------

fn resolve_schema<'a>(spec: &'a OpenAPI, reference: &str) -> Option<&'a Schema> {
    let name = reference.strip_prefix("#/components/schemas/")?;
    match spec.components.as_ref()?.schemas.get(name)? {
        ReferenceOr::Item(s) => Some(s),
        ReferenceOr::Reference { .. } => None,
    }
}

fn resolve_response<'a>(spec: &'a OpenAPI, reference: &str) -> Option<&'a Response> {
    let name = reference.strip_prefix("#/components/responses/")?;
    match spec.components.as_ref()?.responses.get(name)? {
        ReferenceOr::Item(r) => Some(r),
        ReferenceOr::Reference { .. } => None,
    }
}

fn resolve_parameter<'a>(spec: &'a OpenAPI, reference: &str) -> Option<&'a Parameter> {
    let name = reference.strip_prefix("#/components/parameters/")?;
    match spec.components.as_ref()?.parameters.get(name)? {
        ReferenceOr::Item(p) => Some(p),
        ReferenceOr::Reference { .. } => None,
    }
}

fn resolve_request_body<'a>(spec: &'a OpenAPI, reference: &str) -> Option<&'a RequestBody> {
    let name = reference.strip_prefix("#/components/requestBodies/")?;
    match spec.components.as_ref()?.request_bodies.get(name)? {
        ReferenceOr::Item(r) => Some(r),
        ReferenceOr::Reference { .. } => None,
    }
}

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
    serde_yml::from_str(content).map_err(|e| DriftError::Parse(e.to_string()))
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
    // Keys are normalised (template variable names collapsed) so that renaming a
    // path variable (/users/{id} → /users/{userId}) is not seen as remove+add.
    for (norm_key, (display, base_op)) in &base_ops {
        if let Some((_head_display, head_op)) = head_ops.get(norm_key) {
            diff_operation(display, base_op, head_op, base, head, &mut changes);
        } else {
            changes.push(DiffChange {
                path: display.clone(),
                kind: ChangeKind::OperationRemoved,
                severity: Severity::Breaking,
                description: Some(format!("Operation {} was removed", display)),
            });
        }
    }

    // --- Operations added ---------------------------------------------------
    for (norm_key, (display, _head_op)) in &head_ops {
        if !base_ops.contains_key(norm_key) {
            changes.push(DiffChange {
                path: display.clone(),
                kind: ChangeKind::OperationAdded,
                severity: Severity::Safe,
                description: Some(format!("Operation {} was added", display)),
            });
        }
    }

    changes
}

/// Collapse `{templateVar}` segments to `{}` so that path-variable renames
/// (which do not change the contract) do not register as remove + add.
fn normalize_op_key(method_path: &str) -> String {
    let mut out = String::with_capacity(method_path.len());
    let mut in_brace = false;
    for ch in method_path.chars() {
        match ch {
            '{' => {
                in_brace = true;
                out.push('{');
            }
            '}' => {
                in_brace = false;
                out.push('}');
            }
            _ if in_brace => {} // drop the variable name
            _ => out.push(ch),
        }
    }
    out
}

/// True when a schema `prefix` refers to a request body (where required/optional
/// semantics are the mirror image of a response).
fn is_request_context(prefix: &str) -> bool {
    prefix == "request_body" || prefix.starts_with("request_body.")
}

// ---------------------------------------------------------------------------
// Internal helpers — operation collection
// ---------------------------------------------------------------------------

/// Collect all (method + path) → Operation pairs from a spec, keyed by the
/// normalised string "METHOD /path" (template variables collapsed), with the
/// original human-readable "METHOD /path" retained as the display value.
fn collect_operations(spec: &OpenAPI) -> IndexMap<String, (String, openapiv3::Operation)> {
    let mut map = IndexMap::new();

    for (path_str, path_ref) in spec.paths.paths.iter() {
        // Limitation: only local $ref path items (starting with `#/`) are resolved inline.
        // External file refs and URL refs are not supported and will be skipped with a warning.
        let path_item: &PathItem = match path_ref {
            ReferenceOr::Item(pi) => pi,
            ReferenceOr::Reference { reference } => {
                if reference.starts_with("#/") {
                    // Attempt inline resolution of a local $ref path item.
                    // openapiv3 does not expose a generic component resolver for path items,
                    // so we cannot dereference this without a full $ref resolver library.
                    // Log a warning and skip rather than silently losing operations.
                    tracing::warn!(
                        reference = %reference,
                        "skipping local $ref path item (inline $ref resolution for path items is not yet implemented)"
                    );
                } else {
                    tracing::warn!(
                        reference = %reference,
                        "skipping external $ref path item (only local component refs are supported)"
                    );
                }
                continue;
            }
        };

        for (method, op) in path_item.iter() {
            let mut op = op.clone();
            // Fold path-item-level parameters (shared across operations) in front
            // of operation-level ones so shared params participate in the diff.
            // Operation-level params override on key collision (spec semantics)
            // because resolved_params inserts them last.
            if !path_item.parameters.is_empty() {
                let mut merged = path_item.parameters.clone();
                merged.extend(op.parameters.clone());
                op.parameters = merged;
            }
            let display = format!("{} {}", method.to_uppercase(), path_str);
            let norm_key = normalize_op_key(&display);
            map.insert(norm_key, (display, op));
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
    base_spec: &OpenAPI,
    head_spec: &OpenAPI,
    changes: &mut Vec<DiffChange>,
) {
    diff_parameters(op_path, base_op, head_op, base_spec, head_spec, changes);
    diff_request_body(op_path, base_op, head_op, base_spec, head_spec, changes);
    diff_responses(op_path, base_op, head_op, base_spec, head_spec, changes);
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
        Parameter::Path { parameter_data, .. } => (parameter_data.name.clone(), "path".to_string()),
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

/// Collect resolved parameters from an operation, resolving component $refs.
fn resolved_params(op: &openapiv3::Operation, spec: &OpenAPI) -> IndexMap<ParamKey, Parameter> {
    let mut map = IndexMap::new();
    for param_ref in &op.parameters {
        match param_ref {
            ReferenceOr::Item(p) => {
                map.insert(param_key(p), p.clone());
            }
            ReferenceOr::Reference { reference } => {
                if let Some(p) = resolve_parameter(spec, reference) {
                    map.insert(param_key(p), p.clone());
                } else {
                    // Limitation: only local component refs (#/components/parameters/…) are
                    // resolved. External and chain refs are skipped; the parameter is ignored.
                    tracing::warn!(
                        reference = %reference,
                        "could not resolve $ref parameter; parameter will be ignored in diff"
                    );
                }
            }
        }
    }
    map
}

fn diff_parameters(
    op_path: &str,
    base_op: &openapiv3::Operation,
    head_op: &openapiv3::Operation,
    base_spec: &OpenAPI,
    head_spec: &OpenAPI,
    changes: &mut Vec<DiffChange>,
) {
    let base_params = resolved_params(base_op, base_spec);
    let head_params = resolved_params(head_op, head_spec);

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

    // Param removed from head → ParameterRemoved
    for (key, base_p) in &base_params {
        if !head_params.contains_key(key) {
            let (severity, description) = if param_required(base_p) {
                (
                    Severity::Breaking,
                    format!(
                        "Required {} parameter '{}' was removed",
                        key.location, key.name
                    ),
                )
            } else {
                (
                    Severity::NonBreakingRisky,
                    format!(
                        "Optional {} parameter '{}' was removed",
                        key.location, key.name
                    ),
                )
            };
            changes.push(DiffChange {
                path: format!("{} \u{2192} param.{}", op_path, key.name),
                kind: ChangeKind::ParameterRemoved,
                severity,
                description: Some(description),
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

    // Param requiredness changed (present in both) → optional→required is Breaking,
    // required→optional is Safe (relaxation).
    for (key, base_p) in &base_params {
        if let Some(head_p) = head_params.get(key) {
            let was = param_required(base_p);
            let now = param_required(head_p);
            if !was && now {
                changes.push(DiffChange {
                    path: format!("{} \u{2192} param.{}", op_path, key.name),
                    kind: ChangeKind::RequiredChanged,
                    severity: Severity::Breaking,
                    description: Some(format!(
                        "{} parameter '{}' changed from optional to required",
                        key.location, key.name
                    )),
                });
            } else if was && !now {
                changes.push(DiffChange {
                    path: format!("{} \u{2192} param.{}", op_path, key.name),
                    kind: ChangeKind::RequiredChanged,
                    severity: Severity::Safe,
                    description: Some(format!(
                        "{} parameter '{}' changed from required to optional",
                        key.location, key.name
                    )),
                });
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
// Request body diffing
// ---------------------------------------------------------------------------

fn diff_request_body(
    op_path: &str,
    base_op: &openapiv3::Operation,
    head_op: &openapiv3::Operation,
    base_spec: &OpenAPI,
    head_spec: &OpenAPI,
    changes: &mut Vec<DiffChange>,
) {
    let base_rb: Option<&RequestBody> = match base_op.request_body.as_ref() {
        None => None,
        Some(ReferenceOr::Item(rb)) => Some(rb),
        Some(ReferenceOr::Reference { reference }) => {
            if let Some(rb) = resolve_request_body(base_spec, reference) {
                Some(rb)
            } else {
                // Limitation: only local component refs (#/components/requestBodies/…) resolve.
                tracing::warn!(
                    reference = %reference,
                    "could not resolve $ref requestBody; request body diff skipped"
                );
                None
            }
        }
    };
    let head_rb: Option<&RequestBody> = match head_op.request_body.as_ref() {
        None => None,
        Some(ReferenceOr::Item(rb)) => Some(rb),
        Some(ReferenceOr::Reference { reference }) => {
            if let Some(rb) = resolve_request_body(head_spec, reference) {
                Some(rb)
            } else {
                // Limitation: only local component refs (#/components/requestBodies/…) resolve.
                tracing::warn!(
                    reference = %reference,
                    "could not resolve $ref requestBody; request body diff skipped"
                );
                None
            }
        }
    };

    match (base_rb, head_rb) {
        (None, None) => {}
        (Some(_), None) => {
            changes.push(DiffChange {
                path: format!("{} \u{2192} request_body", op_path),
                kind: ChangeKind::RequestBodyRemoved,
                severity: Severity::Breaking,
                description: Some("Request body was removed".to_string()),
            });
        }
        (None, Some(head)) => {
            let severity = if head.required {
                Severity::Breaking
            } else {
                Severity::Safe
            };
            changes.push(DiffChange {
                path: format!("{} \u{2192} request_body", op_path),
                kind: ChangeKind::RequestBodyAdded,
                severity,
                description: Some(if head.required {
                    "Required request body was added".to_string()
                } else {
                    "Optional request body was added".to_string()
                }),
            });
        }
        (Some(base), Some(head)) => {
            // requestBody.required flip: optional→required breaks clients that
            // omit the body; required→optional is a safe relaxation.
            if !base.required && head.required {
                changes.push(DiffChange {
                    path: format!("{} \u{2192} request_body", op_path),
                    kind: ChangeKind::RequiredChanged,
                    severity: Severity::Breaking,
                    description: Some("Request body changed from optional to required".to_string()),
                });
            } else if base.required && !head.required {
                changes.push(DiffChange {
                    path: format!("{} \u{2192} request_body", op_path),
                    kind: ChangeKind::RequiredChanged,
                    severity: Severity::Safe,
                    description: Some("Request body changed from required to optional".to_string()),
                });
            }

            // JSON content type dropped entirely (e.g. switched to XML) → breaks
            // every JSON client. Only flag when the base offered JSON.
            if base.content.contains_key("application/json")
                && !head.content.contains_key("application/json")
            {
                changes.push(DiffChange {
                    path: format!("{} \u{2192} request_body", op_path),
                    kind: ChangeKind::TypeChanged,
                    severity: Severity::Breaking,
                    description: Some(
                        "Request body no longer accepts application/json".to_string(),
                    ),
                });
            }

            if let (Some(base_media), Some(head_media)) = (
                base.content.get("application/json"),
                head.content.get("application/json"),
            ) {
                if let (Some(base_schema_ref), Some(head_schema_ref)) =
                    (&base_media.schema, &head_media.schema)
                {
                    let base_schema: &Schema = match base_schema_ref {
                        ReferenceOr::Item(s) => s,
                        ReferenceOr::Reference { reference } => {
                            match resolve_schema(base_spec, reference) {
                                Some(s) => s,
                                None => {
                                    // Limitation: only local component schema refs resolve.
                                    tracing::warn!(
                                        reference = %reference,
                                        "could not resolve $ref schema in request body; schema diff skipped"
                                    );
                                    return;
                                }
                            }
                        }
                    };
                    let head_schema: &Schema = match head_schema_ref {
                        ReferenceOr::Item(s) => s,
                        ReferenceOr::Reference { reference } => {
                            match resolve_schema(head_spec, reference) {
                                Some(s) => s,
                                None => {
                                    // Limitation: only local component schema refs resolve.
                                    tracing::warn!(
                                        reference = %reference,
                                        "could not resolve $ref schema in request body; schema diff skipped"
                                    );
                                    return;
                                }
                            }
                        }
                    };
                    diff_schema_properties(
                        op_path,
                        "request_body",
                        base_schema,
                        head_schema,
                        base_spec,
                        head_spec,
                        changes,
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Response schema diffing
// ---------------------------------------------------------------------------

fn diff_responses(
    op_path: &str,
    base_op: &openapiv3::Operation,
    head_op: &openapiv3::Operation,
    base_spec: &OpenAPI,
    head_spec: &OpenAPI,
    changes: &mut Vec<DiffChange>,
) {
    let base_responses = &base_op.responses.responses;
    let head_responses = &head_op.responses.responses;

    for (status, base_resp_ref) in base_responses {
        if !is_2xx(status) {
            continue;
        }

        let base_resp: &Response = match base_resp_ref {
            ReferenceOr::Item(r) => r,
            ReferenceOr::Reference { reference } => {
                match resolve_response(base_spec, reference) {
                    Some(r) => r,
                    None => {
                        // Limitation: only local component refs (#/components/responses/…) resolve.
                        tracing::warn!(
                            reference = %reference,
                            "could not resolve $ref response; response diff skipped"
                        );
                        continue;
                    }
                }
            }
        };

        let head_resp: &Response = match head_responses.get(status) {
            Some(ReferenceOr::Item(r)) => r,
            Some(ReferenceOr::Reference { reference }) => {
                match resolve_response(head_spec, reference) {
                    Some(r) => r,
                    None => {
                        // Limitation: only local component refs (#/components/responses/…) resolve.
                        tracing::warn!(
                            reference = %reference,
                            "could not resolve $ref response; response diff skipped"
                        );
                        continue;
                    }
                }
            }
            None => {
                changes.push(DiffChange {
                    path: format!("{} \u{2192} response.{}", op_path, status_code_str(status)),
                    kind: ChangeKind::ResponseRemoved,
                    severity: Severity::Breaking,
                    description: Some(format!(
                        "Response status {} was removed",
                        status_code_str(status)
                    )),
                });
                continue;
            }
        };

        // JSON content type dropped from this response (e.g. switched to XML) →
        // breaks every JSON consumer. Only flag when the base offered JSON.
        if base_resp.content.contains_key("application/json")
            && !head_resp.content.contains_key("application/json")
        {
            changes.push(DiffChange {
                path: format!("{} \u{2192} response.{}", op_path, status_code_str(status)),
                kind: ChangeKind::TypeChanged,
                severity: Severity::Breaking,
                description: Some(format!(
                    "Response {} no longer returns application/json",
                    status_code_str(status)
                )),
            });
        }

        // Compare application/json content schemas
        if let (Some(base_media), Some(head_media)) = (
            base_resp.content.get("application/json"),
            head_resp.content.get("application/json"),
        ) {
            if let (Some(base_schema_ref), Some(head_schema_ref)) =
                (&base_media.schema, &head_media.schema)
            {
                let base_schema: &Schema = match base_schema_ref {
                    ReferenceOr::Item(s) => s,
                    ReferenceOr::Reference { reference } => {
                        match resolve_schema(base_spec, reference) {
                            Some(s) => s,
                            None => {
                                // Limitation: only local component schema refs resolve.
                                tracing::warn!(
                                    reference = %reference,
                                    "could not resolve $ref schema in response; schema diff skipped"
                                );
                                continue;
                            }
                        }
                    }
                };
                let head_schema: &Schema = match head_schema_ref {
                    ReferenceOr::Item(s) => s,
                    ReferenceOr::Reference { reference } => {
                        match resolve_schema(head_spec, reference) {
                            Some(s) => s,
                            None => {
                                // Limitation: only local component schema refs resolve.
                                tracing::warn!(
                                    reference = %reference,
                                    "could not resolve $ref schema in response; schema diff skipped"
                                );
                                continue;
                            }
                        }
                    }
                };

                diff_schema_properties(
                    op_path,
                    "response",
                    base_schema,
                    head_schema,
                    base_spec,
                    head_spec,
                    changes,
                );
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

fn status_code_str(status: &StatusCode) -> String {
    match status {
        StatusCode::Code(n) => n.to_string(),
        StatusCode::Range(n) => format!("{n}XX"),
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
    base_spec: &OpenAPI,
    head_spec: &OpenAPI,
    changes: &mut Vec<DiffChange>,
) {
    // Nullable changes — applies to any schema kind
    let base_nullable = base_schema.schema_data.nullable;
    let head_nullable = head_schema.schema_data.nullable;
    if base_nullable != head_nullable {
        changes.push(DiffChange {
            path: format!("{} \u{2192} {}", op_path, prefix),
            kind: ChangeKind::NullabilityChanged,
            severity: if base_nullable && !head_nullable {
                Severity::Breaking // nullable → non-nullable breaks consumers that send null
            } else {
                Severity::Safe // non-nullable → nullable is more permissive
            },
            description: Some(format!(
                "'{}' changed from {} to {}",
                prefix,
                if base_nullable {
                    "nullable"
                } else {
                    "non-nullable"
                },
                if head_nullable {
                    "nullable"
                } else {
                    "non-nullable"
                },
            )),
        });
    }

    // Enum value changes — applies to string/integer schemas
    diff_enum_values(op_path, prefix, base_schema, head_schema, changes);

    let (base_obj, head_obj) = match (&base_schema.schema_kind, &head_schema.schema_kind) {
        (SchemaKind::Type(Type::Object(b)), SchemaKind::Type(Type::Object(h))) => (b, h),
        _ => return,
    };

    let field_noun = if is_request_context(prefix) {
        "Request property"
    } else {
        "Response property"
    };

    // Properties removed → FieldRemoved (Breaking)
    for (prop_name, _) in &base_obj.properties {
        if !head_obj.properties.contains_key(prop_name) {
            changes.push(DiffChange {
                path: format!("{} \u{2192} {}.{}", op_path, prefix, prop_name),
                kind: ChangeKind::FieldRemoved,
                severity: Severity::Breaking,
                description: Some(format!("{} '{}' was removed", field_noun, prop_name)),
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
                description: Some(format!("{} '{}' was added", field_noun, prop_name)),
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
            ReferenceOr::Reference { reference } => match resolve_schema(base_spec, reference) {
                Some(s) => s,
                None => {
                    // Limitation: only local component schema refs resolve.
                    tracing::warn!(
                        reference = %reference,
                        "could not resolve $ref property schema; property diff skipped"
                    );
                    continue;
                }
            },
        };
        let head_prop_schema: &Schema = match head_prop_ref {
            ReferenceOr::Item(s) => s,
            ReferenceOr::Reference { reference } => match resolve_schema(head_spec, reference) {
                Some(s) => s,
                None => {
                    // Limitation: only local component schema refs resolve.
                    tracing::warn!(
                        reference = %reference,
                        "could not resolve $ref property schema; property diff skipped"
                    );
                    continue;
                }
            },
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

        // Required status changed? Severity is direction-aware: in a REQUEST body,
        // making a field required breaks clients that omit it (Breaking) and
        // relaxing it is Safe; in a RESPONSE, making a field required is Safe and
        // dropping the guarantee (required→optional) is risky for consumers.
        let base_required = base_obj.required.contains(prop_name);
        let head_required = head_obj.required.contains(prop_name);
        let request_ctx = is_request_context(prefix);

        if !base_required && head_required {
            // optional → required
            changes.push(DiffChange {
                path: format!("{} \u{2192} {}.{}", op_path, prefix, prop_name),
                kind: ChangeKind::RequiredChanged,
                severity: if request_ctx {
                    Severity::Breaking
                } else {
                    Severity::Safe
                },
                description: Some(format!(
                    "{} '{}' changed from optional to required",
                    if request_ctx {
                        "Request field"
                    } else {
                        "Response field"
                    },
                    prop_name
                )),
            });
        } else if base_required && !head_required {
            // required → optional
            changes.push(DiffChange {
                path: format!("{} \u{2192} {}.{}", op_path, prefix, prop_name),
                kind: ChangeKind::RequiredChanged,
                severity: if request_ctx {
                    Severity::Safe
                } else {
                    Severity::NonBreakingRisky
                },
                description: Some(format!(
                    "{} '{}' changed from required to optional",
                    if request_ctx {
                        "Request field"
                    } else {
                        "Response field"
                    },
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
            base_spec,
            head_spec,
            changes,
        );
    }
}

fn diff_enum_values(
    op_path: &str,
    prefix: &str,
    base_schema: &Schema,
    head_schema: &Schema,
    changes: &mut Vec<DiffChange>,
) {
    let base_enums = extract_enum_values(base_schema);
    let head_enums = extract_enum_values(head_schema);

    if base_enums.is_empty() && head_enums.is_empty() {
        return;
    }

    for v in &base_enums {
        if !head_enums.contains(v) {
            changes.push(DiffChange {
                path: format!("{} \u{2192} {}", op_path, prefix),
                kind: ChangeKind::EnumValueRemoved,
                severity: Severity::Breaking,
                description: Some(format!("Enum value '{}' was removed from '{}'", v, prefix)),
            });
        }
    }

    for v in &head_enums {
        if !base_enums.contains(v) {
            changes.push(DiffChange {
                path: format!("{} \u{2192} {}", op_path, prefix),
                kind: ChangeKind::EnumValueAdded,
                severity: Severity::NonBreakingRisky,
                description: Some(format!("Enum value '{}' was added to '{}'", v, prefix)),
            });
        }
    }
}

fn extract_enum_values(schema: &Schema) -> Vec<String> {
    match &schema.schema_kind {
        SchemaKind::Type(Type::String(s)) => s
            .enumeration
            .iter()
            .filter_map(|v| v.as_deref().map(String::from))
            .collect(),
        SchemaKind::Type(Type::Integer(i)) => i
            .enumeration
            .iter()
            .filter_map(|v| *v)
            .map(|v| v.to_string())
            .collect(),
        _ => Vec::new(),
    }
}

/// Return a short string describing the primitive type of a schema kind.
/// For arrays, includes the item type: "array<string>", "array<integer>", etc.
/// Returns `None` for complex/compound kinds.
fn type_label_from_kind(kind: &SchemaKind) -> Option<String> {
    match kind {
        SchemaKind::Type(t) => Some(match t {
            Type::String(_) => "string".to_string(),
            Type::Number(_) => "number".to_string(),
            Type::Integer(_) => "integer".to_string(),
            Type::Boolean(_) => "boolean".to_string(),
            Type::Object(_) => "object".to_string(),
            Type::Array(a) => {
                let item_label = a
                    .items
                    .as_ref()
                    .and_then(|items_ref| match items_ref {
                        ReferenceOr::Item(s) => type_label_from_kind(&s.schema_kind),
                        ReferenceOr::Reference { .. } => None,
                    })
                    .unwrap_or_else(|| "any".to_string());
                format!("array<{item_label}>")
            }
        }),
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

        assert_eq!(
            changes.len(),
            1,
            "Expected exactly 1 change, got: {:?}",
            changes
        );
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

        assert_eq!(
            changes.len(),
            1,
            "Expected exactly 1 change, got: {:?}",
            changes
        );
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
            .filter(|c| c.kind == ChangeKind::RequiredChanged && c.severity == Severity::Breaking)
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
    // 8. Required parameter removed → Breaking ParameterRemoved
    // -----------------------------------------------------------------------
    #[test]
    fn test_required_parameter_removed_is_breaking() {
        let base_yaml = r#"
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
"#;
        let base = parse(base_yaml);
        let head = parse(head_yaml);
        let changes = diff_openapi(&base, &head);
        let removed: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::ParameterRemoved && c.severity == Severity::Breaking)
            .collect();
        assert_eq!(
            removed.len(),
            1,
            "Expected 1 ParameterRemoved/Breaking, got: {:?}",
            changes
        );
        assert!(removed[0].path.contains("filter"));
    }

    // -----------------------------------------------------------------------
    // 9. Optional parameter removed → NonBreakingRisky ParameterRemoved
    // -----------------------------------------------------------------------
    #[test]
    fn test_optional_parameter_removed_is_risky() {
        let base_yaml = r#"
openapi: "3.0.0"
info:
  title: Test
  version: "1"
paths:
  /users:
    get:
      parameters:
        - name: sort
          in: query
          required: false
          schema:
            type: string
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
      responses:
        '200':
          description: ok
"#;
        let base = parse(base_yaml);
        let head = parse(head_yaml);
        let changes = diff_openapi(&base, &head);
        let removed: Vec<_> = changes
            .iter()
            .filter(|c| {
                c.kind == ChangeKind::ParameterRemoved && c.severity == Severity::NonBreakingRisky
            })
            .collect();
        assert_eq!(
            removed.len(),
            1,
            "Expected 1 ParameterRemoved/NonBreakingRisky, got: {:?}",
            changes
        );
        assert!(removed[0].path.contains("sort"));
    }

    // -----------------------------------------------------------------------
    // 10. 2xx status code removed from head → Breaking ResponseRemoved
    // -----------------------------------------------------------------------
    #[test]
    fn test_status_code_removed_is_breaking() {
        let base_yaml = r#"
openapi: "3.0.0"
info:
  title: Test
  version: "1"
paths:
  /items:
    post:
      responses:
        '200':
          description: updated
        '201':
          description: created
"#;
        let head_yaml = r#"
openapi: "3.0.0"
info:
  title: Test
  version: "1"
paths:
  /items:
    post:
      responses:
        '200':
          description: updated
"#;
        let base = parse(base_yaml);
        let head = parse(head_yaml);
        let changes = diff_openapi(&base, &head);
        let removed: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::ResponseRemoved && c.severity == Severity::Breaking)
            .collect();
        assert_eq!(
            removed.len(),
            1,
            "Expected 1 ResponseRemoved/Breaking, got: {:?}",
            changes
        );
        assert!(
            removed[0].path.contains("201"),
            "Expected path to mention '201', got: {}",
            removed[0].path
        );
    }

    // -----------------------------------------------------------------------
    // 11. Required request body added → Breaking RequestBodyAdded
    // -----------------------------------------------------------------------
    #[test]
    fn test_required_request_body_added_is_breaking() {
        let base_yaml = r#"
openapi: "3.0.0"
info:
  title: Test
  version: "1"
paths:
  /items:
    post:
      responses:
        '201':
          description: created
"#;
        let head_yaml = r#"
openapi: "3.0.0"
info:
  title: Test
  version: "1"
paths:
  /items:
    post:
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: object
      responses:
        '201':
          description: created
"#;
        let base = parse(base_yaml);
        let head = parse(head_yaml);
        let changes = diff_openapi(&base, &head);
        let added: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::RequestBodyAdded && c.severity == Severity::Breaking)
            .collect();
        assert_eq!(
            added.len(),
            1,
            "Expected 1 RequestBodyAdded/Breaking, got: {:?}",
            changes
        );
    }

    // -----------------------------------------------------------------------
    // 12. Optional request body added → Safe RequestBodyAdded
    // -----------------------------------------------------------------------
    #[test]
    fn test_optional_request_body_added_is_safe() {
        let base_yaml = r#"
openapi: "3.0.0"
info:
  title: Test
  version: "1"
paths:
  /items:
    post:
      responses:
        '201':
          description: created
"#;
        let head_yaml = r#"
openapi: "3.0.0"
info:
  title: Test
  version: "1"
paths:
  /items:
    post:
      requestBody:
        required: false
        content:
          application/json:
            schema:
              type: object
      responses:
        '201':
          description: created
"#;
        let base = parse(base_yaml);
        let head = parse(head_yaml);
        let changes = diff_openapi(&base, &head);
        let added: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::RequestBodyAdded && c.severity == Severity::Safe)
            .collect();
        assert_eq!(
            added.len(),
            1,
            "Expected 1 RequestBodyAdded/Safe, got: {:?}",
            changes
        );
    }

    // -----------------------------------------------------------------------
    // 13. Request body removed → Breaking RequestBodyRemoved
    // -----------------------------------------------------------------------
    #[test]
    fn test_request_body_removed_is_breaking() {
        let base_yaml = r#"
openapi: "3.0.0"
info:
  title: Test
  version: "1"
paths:
  /items:
    post:
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: object
              properties:
                name:
                  type: string
      responses:
        '201':
          description: created
"#;
        let head_yaml = r#"
openapi: "3.0.0"
info:
  title: Test
  version: "1"
paths:
  /items:
    post:
      responses:
        '201':
          description: created
"#;
        let base = parse(base_yaml);
        let head = parse(head_yaml);
        let changes = diff_openapi(&base, &head);
        let removed: Vec<_> = changes
            .iter()
            .filter(|c| {
                c.kind == ChangeKind::RequestBodyRemoved && c.severity == Severity::Breaking
            })
            .collect();
        assert_eq!(
            removed.len(),
            1,
            "Expected 1 RequestBodyRemoved/Breaking, got: {:?}",
            changes
        );
    }

    // -----------------------------------------------------------------------
    // 14. Request body field removed → Breaking FieldRemoved
    // -----------------------------------------------------------------------
    #[test]
    fn test_request_body_field_removed_is_breaking() {
        let base_yaml = r#"
openapi: "3.0.0"
info:
  title: Test
  version: "1"
paths:
  /items:
    post:
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: object
              properties:
                name:
                  type: string
                tags:
                  type: string
      responses:
        '201':
          description: created
"#;
        let head_yaml = r#"
openapi: "3.0.0"
info:
  title: Test
  version: "1"
paths:
  /items:
    post:
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: object
              properties:
                name:
                  type: string
      responses:
        '201':
          description: created
"#;
        let base = parse(base_yaml);
        let head = parse(head_yaml);
        let changes = diff_openapi(&base, &head);
        let removed: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::FieldRemoved && c.severity == Severity::Breaking)
            .collect();
        assert_eq!(
            removed.len(),
            1,
            "Expected 1 FieldRemoved/Breaking, got: {:?}",
            changes
        );
        assert!(
            removed[0].path.contains("tags"),
            "Expected path to mention 'tags', got: {}",
            removed[0].path
        );
    }

    // -----------------------------------------------------------------------
    // 15. Enum value removed → Breaking EnumValueRemoved
    // -----------------------------------------------------------------------
    #[test]
    fn test_enum_value_removed_is_breaking() {
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
                  status:
                    type: string
                    enum: [active, inactive, suspended]
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
                  status:
                    type: string
                    enum: [active, inactive]
"#;
        let base = parse(base_yaml);
        let head = parse(head_yaml);
        let changes = diff_openapi(&base, &head);
        let removed: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::EnumValueRemoved && c.severity == Severity::Breaking)
            .collect();
        assert_eq!(
            removed.len(),
            1,
            "Expected 1 EnumValueRemoved/Breaking, got: {:?}",
            changes
        );
        assert!(
            removed[0]
                .description
                .as_deref()
                .unwrap_or("")
                .contains("suspended"),
            "Expected description to mention 'suspended', got: {:?}",
            removed[0].description
        );
    }

    // -----------------------------------------------------------------------
    // 16. Enum value added → NonBreakingRisky EnumValueAdded
    // -----------------------------------------------------------------------
    #[test]
    fn test_enum_value_added_is_risky() {
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
                  status:
                    type: string
                    enum: [active, inactive]
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
                  status:
                    type: string
                    enum: [active, inactive, pending]
"#;
        let base = parse(base_yaml);
        let head = parse(head_yaml);
        let changes = diff_openapi(&base, &head);
        let added: Vec<_> = changes
            .iter()
            .filter(|c| {
                c.kind == ChangeKind::EnumValueAdded && c.severity == Severity::NonBreakingRisky
            })
            .collect();
        assert_eq!(
            added.len(),
            1,
            "Expected 1 EnumValueAdded/NonBreakingRisky, got: {:?}",
            changes
        );
        assert!(
            added[0]
                .description
                .as_deref()
                .unwrap_or("")
                .contains("pending"),
            "Expected description to mention 'pending', got: {:?}",
            added[0].description
        );
    }

    // -----------------------------------------------------------------------
    // 17. Nullable true→false → Breaking NullabilityChanged
    // -----------------------------------------------------------------------
    #[test]
    fn test_nullable_removed_is_breaking() {
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
                  nickname:
                    type: string
                    nullable: true
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
                  nickname:
                    type: string
"#;
        let base = parse(base_yaml);
        let head = parse(head_yaml);
        let changes = diff_openapi(&base, &head);
        let changed: Vec<_> = changes
            .iter()
            .filter(|c| {
                c.kind == ChangeKind::NullabilityChanged && c.severity == Severity::Breaking
            })
            .collect();
        assert_eq!(
            changed.len(),
            1,
            "Expected 1 NullabilityChanged/Breaking, got: {:?}",
            changes
        );
        assert!(changed[0].path.contains("nickname"));
    }

    // -----------------------------------------------------------------------
    // 18. Nullable false→true → Safe NullabilityChanged
    // -----------------------------------------------------------------------
    #[test]
    fn test_nullable_added_is_safe() {
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
                  nickname:
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
                  nickname:
                    type: string
                    nullable: true
"#;
        let base = parse(base_yaml);
        let head = parse(head_yaml);
        let changes = diff_openapi(&base, &head);
        let changed: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::NullabilityChanged && c.severity == Severity::Safe)
            .collect();
        assert_eq!(
            changed.len(),
            1,
            "Expected 1 NullabilityChanged/Safe, got: {:?}",
            changes
        );
        assert!(changed[0].path.contains("nickname"));
    }

    // -----------------------------------------------------------------------
    // 19. Array item type changed → Breaking TypeChanged
    // -----------------------------------------------------------------------
    #[test]
    fn test_array_item_type_changed_is_breaking() {
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
                  ids:
                    type: array
                    items:
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
                  ids:
                    type: array
                    items:
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
            "Expected 1 TypeChanged/Breaking, got: {:?}",
            changes
        );
        assert!(type_changed[0].path.contains("ids"));
    }

    // -----------------------------------------------------------------------
    // 20. $ref component schema — field removal detected through reference
    // -----------------------------------------------------------------------
    #[test]
    fn test_ref_component_schema_field_removal_detected() {
        let base_yaml = r#"
openapi: "3.0.0"
info:
  title: Test
  version: "1"
paths:
  /users/{id}:
    get:
      responses:
        '200':
          description: ok
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/User'
components:
  schemas:
    User:
      type: object
      properties:
        id:
          type: string
        email:
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
  /users/{id}:
    get:
      responses:
        '200':
          description: ok
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/User'
components:
  schemas:
    User:
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
        let removed: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::FieldRemoved && c.severity == Severity::Breaking)
            .collect();
        assert_eq!(
            removed.len(),
            1,
            "Expected 1 FieldRemoved through $ref, got: {:?}",
            changes
        );
        assert!(
            removed[0].path.contains("phone"),
            "Expected path to mention 'phone', got: {}",
            removed[0].path
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

    // -----------------------------------------------------------------------
    // M-4 B1: request body field optional → required is Breaking
    // -----------------------------------------------------------------------
    #[test]
    fn test_request_field_optional_to_required_is_breaking() {
        let base_yaml = r#"
openapi: "3.0.0"
info: { title: Test, version: "1" }
paths:
  /items:
    post:
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: object
              required: [id]
              properties:
                id: { type: string }
                name: { type: string }
      responses:
        '201': { description: created }
"#;
        let head_yaml = r#"
openapi: "3.0.0"
info: { title: Test, version: "1" }
paths:
  /items:
    post:
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: object
              required: [id, name]
              properties:
                id: { type: string }
                name: { type: string }
      responses:
        '201': { description: created }
"#;
        let changes = diff_openapi(&parse(base_yaml), &parse(head_yaml));
        let breaking: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::RequiredChanged && c.severity == Severity::Breaking)
            .collect();
        assert_eq!(
            breaking.len(),
            1,
            "request field optional→required must be Breaking, got: {:?}",
            changes
        );
        assert!(breaking[0].path.contains("name"));
    }

    // -----------------------------------------------------------------------
    // M-4: request body field required → optional is Safe (relaxation)
    // -----------------------------------------------------------------------
    #[test]
    fn test_request_field_required_to_optional_is_safe() {
        let base_yaml = r#"
openapi: "3.0.0"
info: { title: Test, version: "1" }
paths:
  /items:
    post:
      requestBody:
        content:
          application/json:
            schema:
              type: object
              required: [id, name]
              properties:
                id: { type: string }
                name: { type: string }
      responses:
        '201': { description: created }
"#;
        let head_yaml = r#"
openapi: "3.0.0"
info: { title: Test, version: "1" }
paths:
  /items:
    post:
      requestBody:
        content:
          application/json:
            schema:
              type: object
              required: [id]
              properties:
                id: { type: string }
                name: { type: string }
      responses:
        '201': { description: created }
"#;
        let changes = diff_openapi(&parse(base_yaml), &parse(head_yaml));
        let req: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::RequiredChanged)
            .collect();
        assert_eq!(req.len(), 1, "got: {:?}", changes);
        assert_eq!(req[0].severity, Severity::Safe);
    }

    // -----------------------------------------------------------------------
    // M-4: response field optional → required stays Safe (direction guard)
    // -----------------------------------------------------------------------
    #[test]
    fn test_response_field_optional_to_required_is_safe() {
        let base_yaml = r#"
openapi: "3.0.0"
info: { title: Test, version: "1" }
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
                  id: { type: string }
"#;
        let head_yaml = r#"
openapi: "3.0.0"
info: { title: Test, version: "1" }
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
                required: [id]
                properties:
                  id: { type: string }
"#;
        let changes = diff_openapi(&parse(base_yaml), &parse(head_yaml));
        let req: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::RequiredChanged)
            .collect();
        assert_eq!(req.len(), 1, "got: {:?}", changes);
        assert_eq!(req[0].severity, Severity::Safe);
    }

    // -----------------------------------------------------------------------
    // M-4 B3: query parameter optional → required is Breaking
    // -----------------------------------------------------------------------
    #[test]
    fn test_param_optional_to_required_is_breaking() {
        let base_yaml = r#"
openapi: "3.0.0"
info: { title: Test, version: "1" }
paths:
  /users:
    get:
      parameters:
        - { name: filter, in: query, required: false, schema: { type: string } }
      responses:
        '200': { description: ok }
"#;
        let head_yaml = r#"
openapi: "3.0.0"
info: { title: Test, version: "1" }
paths:
  /users:
    get:
      parameters:
        - { name: filter, in: query, required: true, schema: { type: string } }
      responses:
        '200': { description: ok }
"#;
        let changes = diff_openapi(&parse(base_yaml), &parse(head_yaml));
        let breaking: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::RequiredChanged && c.severity == Severity::Breaking)
            .collect();
        assert_eq!(
            breaking.len(),
            1,
            "param optional→required must be Breaking, got: {:?}",
            changes
        );
        assert!(breaking[0].path.contains("filter"));
    }

    // -----------------------------------------------------------------------
    // M-4 B2: requestBody.required false → true is Breaking
    // -----------------------------------------------------------------------
    #[test]
    fn test_request_body_required_flip_is_breaking() {
        let base_yaml = r#"
openapi: "3.0.0"
info: { title: Test, version: "1" }
paths:
  /items:
    post:
      requestBody:
        required: false
        content:
          application/json:
            schema: { type: object }
      responses:
        '201': { description: created }
"#;
        let head_yaml = r#"
openapi: "3.0.0"
info: { title: Test, version: "1" }
paths:
  /items:
    post:
      requestBody:
        required: true
        content:
          application/json:
            schema: { type: object }
      responses:
        '201': { description: created }
"#;
        let changes = diff_openapi(&parse(base_yaml), &parse(head_yaml));
        let breaking: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::RequiredChanged && c.severity == Severity::Breaking)
            .collect();
        assert_eq!(
            breaking.len(),
            1,
            "requestBody.required flip must be Breaking, got: {:?}",
            changes
        );
    }

    // -----------------------------------------------------------------------
    // M-4 B4: response dropping application/json is Breaking
    // -----------------------------------------------------------------------
    #[test]
    fn test_response_drops_json_is_breaking() {
        let base_yaml = r#"
openapi: "3.0.0"
info: { title: Test, version: "1" }
paths:
  /users:
    get:
      responses:
        '200':
          description: ok
          content:
            application/json:
              schema: { type: object }
"#;
        let head_yaml = r#"
openapi: "3.0.0"
info: { title: Test, version: "1" }
paths:
  /users:
    get:
      responses:
        '200':
          description: ok
          content:
            application/xml:
              schema: { type: object }
"#;
        let changes = diff_openapi(&parse(base_yaml), &parse(head_yaml));
        let breaking: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::TypeChanged && c.severity == Severity::Breaking)
            .collect();
        assert_eq!(
            breaking.len(),
            1,
            "dropping JSON response must be Breaking, got: {:?}",
            changes
        );
    }

    // -----------------------------------------------------------------------
    // M-4 B7: renaming a path template variable is not remove + add
    // -----------------------------------------------------------------------
    #[test]
    fn test_path_template_rename_is_not_operation_change() {
        let base_yaml = r#"
openapi: "3.0.0"
info: { title: Test, version: "1" }
paths:
  /users/{id}:
    get:
      responses:
        '200': { description: ok }
"#;
        let head_yaml = r#"
openapi: "3.0.0"
info: { title: Test, version: "1" }
paths:
  /users/{userId}:
    get:
      responses:
        '200': { description: ok }
"#;
        let changes = diff_openapi(&parse(base_yaml), &parse(head_yaml));
        assert!(
            !changes
                .iter()
                .any(|c| c.kind == ChangeKind::OperationRemoved
                    || c.kind == ChangeKind::OperationAdded),
            "template rename must not produce operation add/remove, got: {:?}",
            changes
        );
    }

    // -----------------------------------------------------------------------
    // M-4 B5: a path-item-level required parameter is diffed
    // -----------------------------------------------------------------------
    #[test]
    fn test_path_level_required_param_removed_is_breaking() {
        let base_yaml = r#"
openapi: "3.0.0"
info: { title: Test, version: "1" }
paths:
  /users:
    parameters:
      - { name: tenant, in: query, required: true, schema: { type: string } }
    get:
      responses:
        '200': { description: ok }
"#;
        let head_yaml = r#"
openapi: "3.0.0"
info: { title: Test, version: "1" }
paths:
  /users:
    get:
      responses:
        '200': { description: ok }
"#;
        let changes = diff_openapi(&parse(base_yaml), &parse(head_yaml));
        let removed: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::ParameterRemoved && c.severity == Severity::Breaking)
            .collect();
        assert_eq!(
            removed.len(),
            1,
            "path-level required param removal must be Breaking, got: {:?}",
            changes
        );
        assert!(removed[0].path.contains("tenant"));
    }
}
