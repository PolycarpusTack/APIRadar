use indexmap::IndexMap;
use openapiv3::{
    AdditionalProperties, ObjectType, OpenAPI, Parameter, PathItem, ReferenceOr, RequestBody,
    Response, Schema, SchemaKind, StatusCode, Type, VariantOrUnknownOrEmpty,
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

/// Resolve a boxed schema reference (used for array `items`) to a `&Schema`,
/// following a single local component `$ref`.
fn resolve_boxed_schema<'a>(
    spec: &'a OpenAPI,
    r: &'a ReferenceOr<Box<Schema>>,
) -> Option<&'a Schema> {
    match r {
        ReferenceOr::Item(s) => Some(s.as_ref()),
        ReferenceOr::Reference { reference } => resolve_schema(spec, reference),
    }
}

/// Resolve a `ReferenceOr<Schema>` (used for composed-schema members) to a
/// `&Schema`, following a single local component `$ref`.
fn resolve_ref_or_schema<'a>(spec: &'a OpenAPI, r: &'a ReferenceOr<Schema>) -> Option<&'a Schema> {
    match r {
        ReferenceOr::Item(s) => Some(s),
        ReferenceOr::Reference { reference } => resolve_schema(spec, reference),
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
            if let (Some(base_type), Some(head_type)) = (
                param_type_label(base_p, base_spec),
                param_type_label(head_p, head_spec),
            ) {
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
/// N-8: a parameter schema routed through a component `$ref` is now resolved so
/// its type participates in the diff (previously refs returned `None`).
fn param_type_label(p: &Parameter, spec: &OpenAPI) -> Option<String> {
    match &p.parameter_data_ref().format {
        openapiv3::ParameterSchemaOrContent::Schema(ReferenceOr::Item(s)) => {
            type_label_from_kind(&s.schema_kind)
        }
        openapiv3::ParameterSchemaOrContent::Schema(ReferenceOr::Reference { reference }) => {
            let s = resolve_schema(spec, reference)?;
            type_label_from_kind(&s.schema_kind)
        }
        openapiv3::ParameterSchemaOrContent::Content(_) => None,
    }
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
                        &mut std::collections::HashSet::new(),
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

        let head_resp: &Response = match find_matching_response(head_responses, status) {
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
                    &mut std::collections::HashSet::new(),
                );
            }
        }
    }
}

/// Find the head response matching a base status, treating a concrete code
/// (`'200'`) and the range covering it (`'2XX'`) as the same slot so a
/// representation change does not read as a false `ResponseRemoved`.
fn find_matching_response<'a>(
    responses: &'a IndexMap<StatusCode, ReferenceOr<Response>>,
    status: &StatusCode,
) -> Option<&'a ReferenceOr<Response>> {
    if let Some(r) = responses.get(status) {
        return Some(r);
    }
    match status {
        StatusCode::Code(n) => {
            let bucket = n / 100;
            responses.iter().find_map(|(k, v)| match k {
                StatusCode::Range(r) if *r == bucket => Some(v),
                _ => None,
            })
        }
        StatusCode::Range(r) => responses.iter().find_map(|(k, v)| match k {
            StatusCode::Code(n) if n / 100 == *r => Some(v),
            _ => None,
        }),
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
#[allow(clippy::too_many_arguments)] // recursive schema walker; args are inherent
fn diff_schema_properties(
    op_path: &str,
    prefix: &str,
    base_schema: &Schema,
    head_schema: &Schema,
    base_spec: &OpenAPI,
    head_spec: &OpenAPI,
    changes: &mut Vec<DiffChange>,
    // Base-schema pointers currently on the recursion path. A recursive `$ref`
    // (e.g. `User.manager -> #/components/schemas/User`) resolves to the same
    // component `Schema` on every level, so a repeat pointer means we are in a
    // cycle — stop recursing to avoid a stack overflow. Sibling reuse of the same
    // component is fine because the pointer is removed on the way back up.
    visited: &mut std::collections::HashSet<*const Schema>,
) {
    let self_ptr = base_schema as *const Schema;
    if !visited.insert(self_ptr) {
        return; // already an ancestor on this path — recursive schema, stop
    }

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

    // N-5: composed schemas (allOf / oneOf / anyOf). These previously fell through
    // to the `_ => return` arm below and were entirely undiffed.
    match (&base_schema.schema_kind, &head_schema.schema_kind) {
        // allOf on either side → flatten each into a merged object and diff those.
        // (A plain object flattens to itself, so a mixed object/allOf pair works.)
        (SchemaKind::AllOf { .. }, _) | (_, SchemaKind::AllOf { .. }) => {
            let mut base_merged = ObjectType::default();
            let mut head_merged = ObjectType::default();
            merge_all_of(
                base_spec,
                base_schema,
                &mut base_merged,
                &mut std::collections::HashSet::new(),
            );
            merge_all_of(
                head_spec,
                head_schema,
                &mut head_merged,
                &mut std::collections::HashSet::new(),
            );
            diff_object_types(
                op_path,
                prefix,
                &base_merged,
                &head_merged,
                base_spec,
                head_spec,
                changes,
                visited,
            );
            visited.remove(&self_ptr);
            return;
        }
        // oneOf / anyOf → variant-position diff with add/remove detection.
        (SchemaKind::OneOf { one_of: bv }, SchemaKind::OneOf { one_of: hv })
        | (SchemaKind::AnyOf { any_of: bv }, SchemaKind::AnyOf { any_of: hv }) => {
            diff_composed_variants(
                op_path, prefix, bv, hv, base_spec, head_spec, changes, visited,
            );
            visited.remove(&self_ptr);
            return;
        }
        _ => {}
    }

    let (base_obj, head_obj) = match (&base_schema.schema_kind, &head_schema.schema_kind) {
        (SchemaKind::Type(Type::Object(b)), SchemaKind::Type(Type::Object(h))) => (b, h),
        // N-4: array-of-objects (e.g. `GET /users -> [User]`). Recurse into the
        // item schemas so item-level field/type changes are detected — previously
        // these were silently ignored on the most common list-endpoint shape.
        (SchemaKind::Type(Type::Array(ba)), SchemaKind::Type(Type::Array(ha))) => {
            if let (Some(bi), Some(hi)) = (ba.items.as_ref(), ha.items.as_ref()) {
                if let (Some(bs), Some(hs)) = (
                    resolve_boxed_schema(base_spec, bi),
                    resolve_boxed_schema(head_spec, hi),
                ) {
                    diff_schema_properties(
                        op_path, prefix, bs, hs, base_spec, head_spec, changes, visited,
                    );
                }
            }
            visited.remove(&self_ptr);
            return;
        }
        _ => {
            visited.remove(&self_ptr);
            return;
        }
    };

    diff_object_types(
        op_path, prefix, base_obj, head_obj, base_spec, head_spec, changes, visited,
    );

    visited.remove(&self_ptr);
}

/// Flatten an `allOf` (or plain object) schema into a single merged `ObjectType`,
/// unioning member properties and `required`. Resolves each member `$ref` and
/// recurses into nested `allOf`. `seen` guards against a recursive `$ref` cycle.
fn merge_all_of(
    spec: &OpenAPI,
    schema: &Schema,
    acc: &mut ObjectType,
    seen: &mut std::collections::HashSet<*const Schema>,
) {
    if !seen.insert(schema as *const Schema) {
        return; // cycle — stop
    }
    match &schema.schema_kind {
        SchemaKind::Type(Type::Object(o)) => {
            for (k, v) in &o.properties {
                acc.properties.insert(k.clone(), v.clone());
            }
            for r in &o.required {
                if !acc.required.contains(r) {
                    acc.required.push(r.clone());
                }
            }
            if acc.additional_properties.is_none() {
                acc.additional_properties = o.additional_properties.clone();
            }
        }
        SchemaKind::AllOf { all_of } => {
            for member in all_of {
                if let Some(s) = resolve_ref_or_schema(spec, member) {
                    merge_all_of(spec, s, acc, seen);
                }
            }
        }
        _ => {}
    }
}

/// Diff the variants of a `oneOf`/`anyOf` pair by position. Matching-position
/// variants are diffed recursively; a removed variant narrows the contract
/// (Breaking) and an added variant widens it (risky, not breaking).
#[allow(clippy::too_many_arguments)]
fn diff_composed_variants(
    op_path: &str,
    prefix: &str,
    base_variants: &[ReferenceOr<Schema>],
    head_variants: &[ReferenceOr<Schema>],
    base_spec: &OpenAPI,
    head_spec: &OpenAPI,
    changes: &mut Vec<DiffChange>,
    visited: &mut std::collections::HashSet<*const Schema>,
) {
    let common = base_variants.len().min(head_variants.len());
    for i in 0..common {
        if let (Some(bs), Some(hs)) = (
            resolve_ref_or_schema(base_spec, &base_variants[i]),
            resolve_ref_or_schema(head_spec, &head_variants[i]),
        ) {
            let variant_prefix = format!("{prefix}[{i}]");
            diff_schema_properties(
                op_path,
                &variant_prefix,
                bs,
                hs,
                base_spec,
                head_spec,
                changes,
                visited,
            );
        }
    }
    // Removed variant(s) — the accepted/returned set shrank → Breaking.
    for i in head_variants.len()..base_variants.len() {
        changes.push(DiffChange {
            path: format!("{op_path} \u{2192} {prefix}[{i}]"),
            kind: ChangeKind::TypeChanged,
            severity: Severity::Breaking,
            description: Some(format!("Variant [{i}] was removed from '{prefix}'")),
        });
    }
    // Added variant(s) — a wider contract → risky but not breaking.
    for i in base_variants.len()..head_variants.len() {
        changes.push(DiffChange {
            path: format!("{op_path} \u{2192} {prefix}[{i}]"),
            kind: ChangeKind::TypeChanged,
            severity: Severity::NonBreakingRisky,
            description: Some(format!("Variant [{i}] was added to '{prefix}'")),
        });
    }
}

/// Diff two object schemas: field add/remove, per-field type/required/constraint
/// changes, `additionalProperties`, and recursion into nested objects.
#[allow(clippy::too_many_arguments)] // object walker; args are inherent
fn diff_object_types(
    op_path: &str,
    prefix: &str,
    base_obj: &ObjectType,
    head_obj: &ObjectType,
    base_spec: &OpenAPI,
    head_spec: &OpenAPI,
    changes: &mut Vec<DiffChange>,
    visited: &mut std::collections::HashSet<*const Schema>,
) {
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

    // N-8: additionalProperties tightened (e.g. true → false) rejects previously
    // accepted/returned extra keys → Breaking; loosened → risky.
    let base_addl = additional_props_label(base_obj);
    let head_addl = additional_props_label(head_obj);
    if base_addl != head_addl {
        let tightened = head_addl == "false" && base_addl != "false";
        changes.push(DiffChange {
            path: format!("{} \u{2192} {}", op_path, prefix),
            kind: ChangeKind::ConstraintChanged,
            severity: if tightened {
                Severity::Breaking
            } else {
                Severity::NonBreakingRisky
            },
            description: Some(format!(
                "additionalProperties changed from '{base_addl}' to '{head_addl}'"
            )),
        });
    }

    // Properties present in both: compare type, requiredness and constraints
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

        let prop_label = format!("{}.{}", prefix, prop_name);

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

        // N-8: format / numeric / string constraint drift.
        diff_scalar_constraints(
            op_path,
            &prop_label,
            base_prop_schema,
            head_prop_schema,
            changes,
        );

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
            visited,
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
// N-8: format / constraint / additionalProperties helpers
// ---------------------------------------------------------------------------

/// A stable label for an object's `additionalProperties` setting.
fn additional_props_label(o: &ObjectType) -> &'static str {
    match &o.additional_properties {
        None => "unset",
        Some(AdditionalProperties::Any(true)) => "true",
        Some(AdditionalProperties::Any(false)) => "false",
        Some(AdditionalProperties::Schema(_)) => "schema",
    }
}

/// Extract a comparable `format` label (e.g. "Int32", "DateTime") from a schema,
/// or `None` when no format is set / the kind carries no format.
fn schema_format_label(schema: &Schema) -> Option<String> {
    match &schema.schema_kind {
        SchemaKind::Type(Type::String(s)) => variant_label(&s.format),
        SchemaKind::Type(Type::Integer(i)) => variant_label(&i.format),
        SchemaKind::Type(Type::Number(n)) => variant_label(&n.format),
        _ => None,
    }
}

fn variant_label<T: std::fmt::Debug>(f: &VariantOrUnknownOrEmpty<T>) -> Option<String> {
    match f {
        VariantOrUnknownOrEmpty::Empty => None,
        VariantOrUnknownOrEmpty::Item(x) => Some(format!("{x:?}")),
        VariantOrUnknownOrEmpty::Unknown(s) => Some(s.clone()),
    }
}

/// Detect `format`, numeric/length and `pattern` constraint drift on a scalar
/// property. A tightened constraint (or a new/changed pattern) is Breaking; a
/// loosened one is risky.
fn diff_scalar_constraints(
    op_path: &str,
    label: &str,
    base: &Schema,
    head: &Schema,
    changes: &mut Vec<DiffChange>,
) {
    // format (int32→int64, date→date-time, …)
    let bf = schema_format_label(base);
    let hf = schema_format_label(head);
    if bf != hf && (bf.is_some() || hf.is_some()) {
        changes.push(DiffChange {
            path: format!("{op_path} \u{2192} {label}"),
            kind: ChangeKind::ConstraintChanged,
            severity: Severity::Breaking,
            description: Some(format!(
                "'{label}' format changed from {} to {}",
                bf.as_deref().unwrap_or("none"),
                hf.as_deref().unwrap_or("none")
            )),
        });
    }

    match (&base.schema_kind, &head.schema_kind) {
        (SchemaKind::Type(Type::String(b)), SchemaKind::Type(Type::String(h))) => {
            push_bound_change(
                op_path,
                label,
                "maxLength",
                b.max_length.map(|v| v as i128),
                h.max_length.map(|v| v as i128),
                true,
                changes,
            );
            push_bound_change(
                op_path,
                label,
                "minLength",
                b.min_length.map(|v| v as i128),
                h.min_length.map(|v| v as i128),
                false,
                changes,
            );
            if b.pattern != h.pattern {
                // Any pattern change/addition can reject previously valid values.
                changes.push(DiffChange {
                    path: format!("{op_path} \u{2192} {label}"),
                    kind: ChangeKind::ConstraintChanged,
                    severity: Severity::Breaking,
                    description: Some(format!(
                        "'{label}' pattern changed from {:?} to {:?}",
                        b.pattern, h.pattern
                    )),
                });
            }
        }
        (SchemaKind::Type(Type::Integer(b)), SchemaKind::Type(Type::Integer(h))) => {
            push_bound_change(
                op_path,
                label,
                "minimum",
                b.minimum.map(i128::from),
                h.minimum.map(i128::from),
                false,
                changes,
            );
            push_bound_change(
                op_path,
                label,
                "maximum",
                b.maximum.map(i128::from),
                h.maximum.map(i128::from),
                true,
                changes,
            );
        }
        (SchemaKind::Type(Type::Number(b)), SchemaKind::Type(Type::Number(h))) => {
            push_bound_change_f64(
                op_path, label, "minimum", b.minimum, h.minimum, false, changes,
            );
            push_bound_change_f64(
                op_path, label, "maximum", b.maximum, h.maximum, true, changes,
            );
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn push_bound_change(
    op_path: &str,
    label: &str,
    name: &str,
    base: Option<i128>,
    head: Option<i128>,
    is_upper_bound: bool,
    changes: &mut Vec<DiffChange>,
) {
    if base == head {
        return;
    }
    let tightened = match (base, head) {
        (None, Some(_)) => true,  // adding a bound restricts the accepted set
        (Some(_), None) => false, // removing a bound relaxes it
        (Some(b), Some(h)) => {
            if is_upper_bound {
                h < b
            } else {
                h > b
            }
        }
        (None, None) => return,
    };
    changes.push(DiffChange {
        path: format!("{op_path} \u{2192} {label}"),
        kind: ChangeKind::ConstraintChanged,
        severity: if tightened {
            Severity::Breaking
        } else {
            Severity::NonBreakingRisky
        },
        description: Some(format!(
            "'{label}' {name} changed from {} to {}",
            base.map(|v| v.to_string())
                .unwrap_or_else(|| "unset".into()),
            head.map(|v| v.to_string())
                .unwrap_or_else(|| "unset".into()),
        )),
    });
}

#[allow(clippy::too_many_arguments)]
fn push_bound_change_f64(
    op_path: &str,
    label: &str,
    name: &str,
    base: Option<f64>,
    head: Option<f64>,
    is_upper_bound: bool,
    changes: &mut Vec<DiffChange>,
) {
    if base == head {
        return;
    }
    let tightened = match (base, head) {
        (None, Some(_)) => true,
        (Some(_), None) => false,
        (Some(b), Some(h)) => {
            if is_upper_bound {
                h < b
            } else {
                h > b
            }
        }
        (None, None) => return,
    };
    changes.push(DiffChange {
        path: format!("{op_path} \u{2192} {label}"),
        kind: ChangeKind::ConstraintChanged,
        severity: if tightened {
            Severity::Breaking
        } else {
            Severity::NonBreakingRisky
        },
        description: Some(format!(
            "'{label}' {name} changed from {} to {}",
            base.map(|v| v.to_string())
                .unwrap_or_else(|| "unset".into()),
            head.map(|v| v.to_string())
                .unwrap_or_else(|| "unset".into()),
        )),
    });
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

    // -----------------------------------------------------------------------
    // N-1: a recursive $ref schema must not stack-overflow
    // -----------------------------------------------------------------------
    #[test]
    fn test_recursive_ref_schema_does_not_overflow() {
        let base_yaml = r#"
openapi: "3.0.0"
info: { title: Test, version: "1" }
paths:
  /users/{id}:
    get:
      responses:
        '200':
          description: ok
          content:
            application/json:
              schema: { $ref: '#/components/schemas/User' }
components:
  schemas:
    User:
      type: object
      properties:
        id: { type: string }
        manager: { $ref: '#/components/schemas/User' }
"#;
        // Head is identical except the id type changes — this forces recursion
        // into the self-referential `manager` chain while still having a change
        // to detect at the top level.
        let head_yaml = base_yaml.replace("id: { type: string }", "id: { type: integer }");
        let base = parse(base_yaml);
        let head = parse(&head_yaml);

        // Must return (not stack-overflow) and detect the id type change exactly once.
        let changes = diff_openapi(&base, &head);
        let type_changed: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::TypeChanged && c.path.contains("id"))
            .collect();
        assert_eq!(
            type_changed.len(),
            1,
            "recursive schema must be diffed once without overflow, got: {:?}",
            changes
        );
    }

    // -----------------------------------------------------------------------
    // N-4: array-of-object responses are diffed at the item level
    // -----------------------------------------------------------------------
    #[test]
    fn test_array_item_field_removed_is_breaking() {
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
                type: array
                items:
                  type: object
                  properties:
                    id: { type: string }
                    phone: { type: string }
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
                type: array
                items:
                  type: object
                  properties:
                    id: { type: string }
"#;
        let changes = diff_openapi(&parse(base_yaml), &parse(head_yaml));
        let removed: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::FieldRemoved && c.severity == Severity::Breaking)
            .collect();
        assert_eq!(
            removed.len(),
            1,
            "array-item field removal must be Breaking, got: {:?}",
            changes
        );
        assert!(removed[0].path.contains("phone"));
    }

    #[test]
    fn test_array_item_type_changed_via_ref_is_breaking() {
        // Array items referenced via $ref; an item field's type changes.
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
                type: array
                items: { $ref: '#/components/schemas/User' }
components:
  schemas:
    User:
      type: object
      properties:
        id: { type: string }
"#;
        let head_yaml = base_yaml.replace("id: { type: string }", "id: { type: integer }");
        let changes = diff_openapi(&parse(base_yaml), &parse(&head_yaml));
        assert!(
            changes
                .iter()
                .any(|c| c.kind == ChangeKind::TypeChanged && c.path.contains("id")),
            "array item type change via $ref must be detected, got: {:?}",
            changes
        );
    }

    // -----------------------------------------------------------------------
    // N-5: allOf-composed schema — a field removed from one member is Breaking
    // -----------------------------------------------------------------------
    #[test]
    fn test_allof_member_field_removed_is_breaking() {
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
                allOf:
                  - $ref: '#/components/schemas/Base'
                  - type: object
                    properties:
                      extra: { type: string }
components:
  schemas:
    Base:
      type: object
      properties:
        id: { type: string }
        phone: { type: string }
"#;
        let head_yaml = base_yaml.replace("        phone: { type: string }\n", "");
        let changes = diff_openapi(&parse(base_yaml), &parse(&head_yaml));
        let removed: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::FieldRemoved && c.severity == Severity::Breaking)
            .collect();
        assert_eq!(
            removed.len(),
            1,
            "allOf member field removal must be Breaking, got: {:?}",
            changes
        );
        assert!(removed[0].path.contains("phone"));
    }

    // -----------------------------------------------------------------------
    // N-5: oneOf variant removed → Breaking
    // -----------------------------------------------------------------------
    #[test]
    fn test_oneof_variant_removed_is_breaking() {
        let base_yaml = r#"
openapi: "3.0.0"
info: { title: Test, version: "1" }
paths:
  /pets:
    get:
      responses:
        '200':
          description: ok
          content:
            application/json:
              schema:
                oneOf:
                  - type: object
                    properties:
                      a: { type: string }
                  - type: object
                    properties:
                      b: { type: string }
"#;
        let head_yaml = r#"
openapi: "3.0.0"
info: { title: Test, version: "1" }
paths:
  /pets:
    get:
      responses:
        '200':
          description: ok
          content:
            application/json:
              schema:
                oneOf:
                  - type: object
                    properties:
                      a: { type: string }
"#;
        let changes = diff_openapi(&parse(base_yaml), &parse(head_yaml));
        let breaking: Vec<_> = changes
            .iter()
            .filter(|c| c.severity == Severity::Breaking)
            .collect();
        assert_eq!(
            breaking.len(),
            1,
            "oneOf variant removal must be Breaking, got: {:?}",
            changes
        );
    }

    // -----------------------------------------------------------------------
    // N-8 helpers: build a spec with a single response property of the given
    // schema snippet, for concise constraint tests.
    // -----------------------------------------------------------------------
    fn spec_with_prop(prop_schema: &str) -> String {
        format!(
            r#"
openapi: "3.0.0"
info: {{ title: Test, version: "1" }}
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
                  field:
{}
"#,
            prop_schema
        )
    }

    // N-8: format change int32 → int64 → Breaking ConstraintChanged
    #[test]
    fn test_format_change_is_breaking() {
        let base =
            spec_with_prop("                    type: integer\n                    format: int32");
        let head =
            spec_with_prop("                    type: integer\n                    format: int64");
        let changes = diff_openapi(&parse(&base), &parse(&head));
        let c: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::ConstraintChanged && c.severity == Severity::Breaking)
            .collect();
        assert_eq!(
            c.len(),
            1,
            "format change must be Breaking, got: {:?}",
            changes
        );
    }

    // N-8: maxLength tightened → Breaking ConstraintChanged
    #[test]
    fn test_maxlength_tightened_is_breaking() {
        let base =
            spec_with_prop("                    type: string\n                    maxLength: 10");
        let head =
            spec_with_prop("                    type: string\n                    maxLength: 5");
        let changes = diff_openapi(&parse(&base), &parse(&head));
        let c: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::ConstraintChanged && c.severity == Severity::Breaking)
            .collect();
        assert_eq!(
            c.len(),
            1,
            "maxLength tightening must be Breaking, got: {:?}",
            changes
        );
    }

    // N-8: minimum raised → Breaking ConstraintChanged
    #[test]
    fn test_minimum_raised_is_breaking() {
        let base =
            spec_with_prop("                    type: integer\n                    minimum: 0");
        let head =
            spec_with_prop("                    type: integer\n                    minimum: 18");
        let changes = diff_openapi(&parse(&base), &parse(&head));
        let c: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::ConstraintChanged && c.severity == Severity::Breaking)
            .collect();
        assert_eq!(
            c.len(),
            1,
            "minimum raise must be Breaking, got: {:?}",
            changes
        );
    }

    // N-8: pattern added → Breaking ConstraintChanged
    #[test]
    fn test_pattern_added_is_breaking() {
        let base = spec_with_prop("                    type: string");
        let head = spec_with_prop(
            "                    type: string\n                    pattern: '^[a-z]+$'",
        );
        let changes = diff_openapi(&parse(&base), &parse(&head));
        let c: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::ConstraintChanged && c.severity == Severity::Breaking)
            .collect();
        assert_eq!(
            c.len(),
            1,
            "pattern addition must be Breaking, got: {:?}",
            changes
        );
    }

    // N-8: additionalProperties true → false → Breaking ConstraintChanged
    #[test]
    fn test_additional_properties_restricted_is_breaking() {
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
                additionalProperties: true
                properties:
                  id: { type: string }
"#;
        let head_yaml =
            base_yaml.replace("additionalProperties: true", "additionalProperties: false");
        let changes = diff_openapi(&parse(base_yaml), &parse(&head_yaml));
        let c: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::ConstraintChanged && c.severity == Severity::Breaking)
            .collect();
        assert_eq!(
            c.len(),
            1,
            "additionalProperties true→false must be Breaking, got: {:?}",
            changes
        );
    }

    // N-8: a parameter whose type is routed through a $ref changes → Breaking
    #[test]
    fn test_param_type_change_via_ref_is_breaking() {
        let base_yaml = r#"
openapi: "3.0.0"
info: { title: Test, version: "1" }
paths:
  /users:
    get:
      parameters:
        - name: id
          in: query
          schema: { $ref: '#/components/schemas/IdType' }
      responses:
        '200': { description: ok }
components:
  schemas:
    IdType: { type: string }
"#;
        let head_yaml = base_yaml.replace("IdType: { type: string }", "IdType: { type: integer }");
        let changes = diff_openapi(&parse(base_yaml), &parse(&head_yaml));
        let c: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::TypeChanged && c.severity == Severity::Breaking)
            .collect();
        assert_eq!(
            c.len(),
            1,
            "param type change via $ref must be Breaking, got: {:?}",
            changes
        );
        assert!(c[0].path.contains("id"));
    }

    // N-8: '200' vs '2XX' must not read as a false ResponseRemoved
    #[test]
    fn test_status_code_200_vs_2xx_no_false_removal() {
        let base_yaml = r#"
openapi: "3.0.0"
info: { title: Test, version: "1" }
paths:
  /users:
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
        '2XX': { description: ok }
"#;
        let changes = diff_openapi(&parse(base_yaml), &parse(head_yaml));
        assert!(
            !changes
                .iter()
                .any(|c| c.kind == ChangeKind::ResponseRemoved),
            "'200' vs '2XX' must not be a ResponseRemoved, got: {:?}",
            changes
        );
    }
}
