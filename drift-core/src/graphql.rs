use std::collections::HashMap;

use graphql_parser::schema::{
    self, Definition, EnumType, Field, InputObjectType, InputValue, InterfaceType, ObjectType,
    TypeDefinition, UnionType,
};

use crate::{
    diff::DiffChange,
    error::DriftError,
    models::{ChangeKind, Severity},
};

// ---------------------------------------------------------------------------
// Intermediate representation — owned, lifetime-free
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct GqlField {
    pub name: String,
    pub type_str: String,
    pub arguments: Vec<GqlArg>,
}

#[derive(Debug, Clone)]
pub struct GqlArg {
    pub name: String,
    pub type_str: String,
    pub has_default: bool,
}

#[derive(Debug, Clone)]
pub enum GqlTypeKind {
    Object { fields: Vec<GqlField> },
    Interface { fields: Vec<GqlField> },
    InputObject { fields: Vec<GqlField> },
    Enum { values: Vec<String> },
    Union { members: Vec<String> },
    Scalar,
}

#[derive(Debug, Clone)]
pub struct GqlType {
    pub name: String,
    pub kind: GqlTypeKind,
}

pub type TypeMap = HashMap<String, GqlType>;

// ---------------------------------------------------------------------------
// parse_graphql
// ---------------------------------------------------------------------------

/// Parse a GraphQL SDL string into a TypeMap.
pub fn parse_graphql(content: &str) -> Result<TypeMap, DriftError> {
    let doc = schema::parse_schema::<String>(content)
        .map_err(|e| DriftError::Parse(format!("GraphQL: {e}")))?;

    let mut map = TypeMap::new();
    for def in doc.definitions {
        if let Definition::TypeDefinition(td) = def {
            let gql_type = convert_type_def(td);
            if !is_builtin(&gql_type.name) {
                map.insert(gql_type.name.clone(), gql_type);
            }
        }
    }
    Ok(map)
}

fn type_to_str(t: &graphql_parser::query::Type<String>) -> String {
    match t {
        graphql_parser::query::Type::NamedType(name) => name.clone(),
        graphql_parser::query::Type::ListType(inner) => format!("[{}]", type_to_str(inner)),
        graphql_parser::query::Type::NonNullType(inner) => format!("{}!", type_to_str(inner)),
    }
}

fn field_to_gql<'a>(f: Field<'a, String>) -> GqlField {
    GqlField {
        name: f.name,
        type_str: type_to_str(&f.field_type),
        arguments: f.arguments.into_iter().map(input_value_to_arg).collect(),
    }
}

fn input_value_to_arg<'a>(iv: InputValue<'a, String>) -> GqlArg {
    GqlArg {
        name: iv.name,
        type_str: type_to_str(&iv.value_type),
        has_default: iv.default_value.is_some(),
    }
}

fn input_value_to_field<'a>(iv: InputValue<'a, String>) -> GqlField {
    GqlField {
        name: iv.name,
        type_str: type_to_str(&iv.value_type),
        arguments: vec![],
    }
}

fn convert_type_def<'a>(td: TypeDefinition<'a, String>) -> GqlType {
    match td {
        TypeDefinition::Object(ObjectType { name, fields, .. }) => GqlType {
            name,
            kind: GqlTypeKind::Object {
                fields: fields.into_iter().map(field_to_gql).collect(),
            },
        },
        TypeDefinition::Interface(InterfaceType { name, fields, .. }) => GqlType {
            name,
            kind: GqlTypeKind::Interface {
                fields: fields.into_iter().map(field_to_gql).collect(),
            },
        },
        TypeDefinition::InputObject(InputObjectType { name, fields, .. }) => GqlType {
            name,
            kind: GqlTypeKind::InputObject {
                fields: fields.into_iter().map(input_value_to_field).collect(),
            },
        },
        TypeDefinition::Enum(EnumType { name, values, .. }) => GqlType {
            name,
            kind: GqlTypeKind::Enum {
                values: values.into_iter().map(|v| v.name).collect(),
            },
        },
        TypeDefinition::Union(UnionType { name, types, .. }) => GqlType {
            name,
            kind: GqlTypeKind::Union { members: types },
        },
        TypeDefinition::Scalar(s) => GqlType {
            name: s.name,
            kind: GqlTypeKind::Scalar,
        },
    }
}

fn is_builtin(name: &str) -> bool {
    matches!(
        name,
        "String"
            | "Int"
            | "Float"
            | "Boolean"
            | "ID"
            | "__Schema"
            | "__Type"
            | "__Field"
            | "__InputValue"
            | "__EnumValue"
            | "__Directive"
            | "__DirectiveLocation"
    )
}

// ---------------------------------------------------------------------------
// diff_graphql
// ---------------------------------------------------------------------------

pub fn diff_graphql(base: &TypeMap, head: &TypeMap) -> Vec<DiffChange> {
    let mut changes = Vec::new();

    for (name, base_type) in base {
        match head.get(name) {
            None => changes.push(DiffChange {
                path: format!("type {name}"),
                kind: ChangeKind::OperationRemoved,
                severity: Severity::Breaking,
                description: Some(format!("Type '{name}' was removed")),
            }),
            Some(head_type) => diff_type(base_type, head_type, &mut changes),
        }
    }

    for name in head.keys() {
        if !base.contains_key(name) {
            changes.push(DiffChange {
                path: format!("type {name}"),
                kind: ChangeKind::OperationAdded,
                severity: Severity::Safe,
                description: Some(format!("Type '{name}' was added")),
            });
        }
    }

    changes
}

fn diff_type(base: &GqlType, head: &GqlType, changes: &mut Vec<DiffChange>) {
    match (&base.kind, &head.kind) {
        (GqlTypeKind::Object { fields: bf }, GqlTypeKind::Object { fields: hf })
        | (GqlTypeKind::Interface { fields: bf }, GqlTypeKind::Interface { fields: hf })
        | (GqlTypeKind::InputObject { fields: bf }, GqlTypeKind::InputObject { fields: hf }) => {
            diff_fields(&base.name, bf, hf, changes);
        }
        (GqlTypeKind::Enum { values: bv }, GqlTypeKind::Enum { values: hv }) => {
            diff_enum_values(&base.name, bv, hv, changes);
        }
        (GqlTypeKind::Union { members: bm }, GqlTypeKind::Union { members: hm }) => {
            diff_union_members(&base.name, bm, hm, changes);
        }
        _ => {
            changes.push(DiffChange {
                path: format!("type {}", base.name),
                kind: ChangeKind::TypeChanged,
                severity: Severity::Breaking,
                description: Some(format!("Type '{}' kind changed", base.name)),
            });
        }
    }
}

fn diff_fields(
    type_name: &str,
    base_fields: &[GqlField],
    head_fields: &[GqlField],
    changes: &mut Vec<DiffChange>,
) {
    let base_map: HashMap<&str, &GqlField> =
        base_fields.iter().map(|f| (f.name.as_str(), f)).collect();
    let head_map: HashMap<&str, &GqlField> =
        head_fields.iter().map(|f| (f.name.as_str(), f)).collect();

    for (name, base_f) in &base_map {
        match head_map.get(name) {
            None => changes.push(DiffChange {
                path: format!("{type_name}.{name}"),
                kind: ChangeKind::FieldRemoved,
                severity: Severity::Breaking,
                description: Some(format!("Field '{type_name}.{name}' was removed")),
            }),
            Some(head_f) => {
                if base_f.type_str != head_f.type_str {
                    changes.push(DiffChange {
                        path: format!("{type_name}.{name}"),
                        kind: ChangeKind::TypeChanged,
                        severity: Severity::Breaking,
                        description: Some(format!(
                            "Field '{type_name}.{name}' type changed from '{}' to '{}'",
                            base_f.type_str, head_f.type_str
                        )),
                    });
                }
                diff_args(type_name, name, &base_f.arguments, &head_f.arguments, changes);
            }
        }
    }

    for name in head_map.keys() {
        if !base_map.contains_key(name) {
            changes.push(DiffChange {
                path: format!("{type_name}.{name}"),
                kind: ChangeKind::FieldAdded,
                severity: Severity::Safe,
                description: Some(format!("Field '{type_name}.{name}' was added")),
            });
        }
    }
}

fn diff_args(
    type_name: &str,
    field_name: &str,
    base_args: &[GqlArg],
    head_args: &[GqlArg],
    changes: &mut Vec<DiffChange>,
) {
    let base_map: HashMap<&str, &GqlArg> =
        base_args.iter().map(|a| (a.name.as_str(), a)).collect();
    let head_map: HashMap<&str, &GqlArg> =
        head_args.iter().map(|a| (a.name.as_str(), a)).collect();

    for name in base_map.keys() {
        if !head_map.contains_key(name) {
            changes.push(DiffChange {
                path: format!("{type_name}.{field_name}({name}:)"),
                kind: ChangeKind::FieldRemoved,
                severity: Severity::Breaking,
                description: Some(format!(
                    "Argument '{type_name}.{field_name}({name})' was removed"
                )),
            });
        }
    }

    for (name, head_arg) in &head_map {
        if !base_map.contains_key(name) {
            let required = head_arg.type_str.ends_with('!') && !head_arg.has_default;
            changes.push(DiffChange {
                path: format!("{type_name}.{field_name}({name}:)"),
                kind: ChangeKind::RequiredChanged,
                severity: if required { Severity::Breaking } else { Severity::Safe },
                description: Some(format!(
                    "Argument '{type_name}.{field_name}({name})' was added{}",
                    if required { " (required)" } else { "" }
                )),
            });
        }
    }
}

fn diff_enum_values(
    type_name: &str,
    base_vals: &[String],
    head_vals: &[String],
    changes: &mut Vec<DiffChange>,
) {
    let base_set: std::collections::HashSet<&str> =
        base_vals.iter().map(|v| v.as_str()).collect();
    let head_set: std::collections::HashSet<&str> =
        head_vals.iter().map(|v| v.as_str()).collect();

    for val in base_set.difference(&head_set) {
        changes.push(DiffChange {
            path: format!("{type_name}.{val}"),
            kind: ChangeKind::FieldRemoved,
            severity: Severity::Breaking,
            description: Some(format!("Enum value '{type_name}.{val}' was removed")),
        });
    }
    for val in head_set.difference(&base_set) {
        changes.push(DiffChange {
            path: format!("{type_name}.{val}"),
            kind: ChangeKind::FieldAdded,
            severity: Severity::Safe,
            description: Some(format!("Enum value '{type_name}.{val}' was added")),
        });
    }
}

fn diff_union_members(
    type_name: &str,
    base_members: &[String],
    head_members: &[String],
    changes: &mut Vec<DiffChange>,
) {
    let base_set: std::collections::HashSet<&str> =
        base_members.iter().map(|m| m.as_str()).collect();
    let head_set: std::collections::HashSet<&str> =
        head_members.iter().map(|m| m.as_str()).collect();

    for m in base_set.difference(&head_set) {
        changes.push(DiffChange {
            path: format!("{type_name} | {m}"),
            kind: ChangeKind::FieldRemoved,
            severity: Severity::Breaking,
            description: Some(format!("Union member '{m}' removed from '{type_name}'")),
        });
    }
    for m in head_set.difference(&base_set) {
        changes.push(DiffChange {
            path: format!("{type_name} | {m}"),
            kind: ChangeKind::FieldAdded,
            severity: Severity::Safe,
            description: Some(format!("Union member '{m}' added to '{type_name}'")),
        });
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(sdl: &str) -> TypeMap {
        parse_graphql(sdl).expect("parse failed")
    }

    #[test]
    fn test_identical_sdl_no_changes() {
        let sdl = r#"
            type User { id: ID!, name: String! }
        "#;
        let m = parse(sdl);
        let changes = diff_graphql(&m, &m);
        assert!(changes.is_empty(), "unexpected changes: {changes:?}");
    }

    #[test]
    fn test_type_removed_is_breaking() {
        let base = parse("type User { id: ID! }");
        let head = parse("type Product { sku: String! }");
        let changes = diff_graphql(&base, &head);
        let removed: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::OperationRemoved && c.severity == Severity::Breaking)
            .collect();
        assert_eq!(removed.len(), 1);
        assert!(removed[0].path.contains("User"));
    }

    #[test]
    fn test_field_removed_is_breaking() {
        let base = parse("type User { id: ID!, phone: String }");
        let head = parse("type User { id: ID! }");
        let changes = diff_graphql(&base, &head);
        let removed: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::FieldRemoved && c.severity == Severity::Breaking)
            .collect();
        assert_eq!(removed.len(), 1, "expected 1 FieldRemoved, got {changes:?}");
        assert!(removed[0].path.contains("phone"));
    }

    #[test]
    fn test_field_added_is_safe() {
        let base = parse("type User { id: ID! }");
        let head = parse("type User { id: ID!, email: String }");
        let changes = diff_graphql(&base, &head);
        let added: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::FieldAdded && c.severity == Severity::Safe)
            .collect();
        assert_eq!(added.len(), 1);
        assert!(added[0].path.contains("email"));
    }

    #[test]
    fn test_field_type_changed_is_breaking() {
        let base = parse("type User { id: ID! }");
        let head = parse("type User { id: Int! }");
        let changes = diff_graphql(&base, &head);
        let type_changed: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::TypeChanged && c.severity == Severity::Breaking)
            .collect();
        assert_eq!(type_changed.len(), 1, "got {changes:?}");
    }

    #[test]
    fn test_enum_value_removed_is_breaking() {
        let base = parse("enum Status { ACTIVE INACTIVE PENDING }");
        let head = parse("enum Status { ACTIVE INACTIVE }");
        let changes = diff_graphql(&base, &head);
        let removed: Vec<_> = changes
            .iter()
            .filter(|c| c.kind == ChangeKind::FieldRemoved && c.severity == Severity::Breaking)
            .collect();
        assert_eq!(removed.len(), 1);
        assert!(removed[0].path.contains("PENDING"));
    }

    #[test]
    fn test_required_argument_added_is_breaking() {
        let base = parse("type Query { user(id: ID!): String }");
        let head = parse("type Query { user(id: ID!, filter: String!): String }");
        let changes = diff_graphql(&base, &head);
        let breaking: Vec<_> = changes
            .iter()
            .filter(|c| c.severity == Severity::Breaking)
            .collect();
        assert_eq!(breaking.len(), 1, "got {changes:?}");
        assert!(breaking[0].path.contains("filter"));
    }
}
