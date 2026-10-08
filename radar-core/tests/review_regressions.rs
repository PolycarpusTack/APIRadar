use radar_core::diff::{diff_openapi, parse_openapi, DiffChange};
use radar_core::graphql::{diff_graphql, parse_graphql};
use radar_core::{ChangeKind, Severity};

fn request_spec(schema: &str) -> String {
    format!(
        r#"openapi: 3.0.0
info: {{title: Review, version: '1'}}
paths:
  /items:
    post:
      requestBody:
        content:
          application/json:
            schema: {schema}
      responses:
        '200': {{description: ok}}
"#
    )
}

fn response_spec(schema: &str) -> String {
    format!(
        r#"openapi: 3.0.0
info: {{title: Review, version: '1'}}
paths:
  /items:
    get:
      responses:
        '200':
          description: ok
          content:
            application/json:
              schema: {schema}
"#
    )
}

fn openapi_changes(base: &str, head: &str) -> Vec<DiffChange> {
    diff_openapi(&parse_openapi(base).unwrap(), &parse_openapi(head).unwrap())
}

fn gql_changes(base: &str, head: &str) -> Vec<DiffChange> {
    diff_graphql(&parse_graphql(base).unwrap(), &parse_graphql(head).unwrap())
}

#[test]
fn added_required_request_property_is_a_breaking_change() {
    let changes = openapi_changes(
        &request_spec("{type: object, properties: {}}"),
        &request_spec("{type: object, properties: {name: {type: string}}, required: [name]}"),
    );
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].kind, ChangeKind::FieldAdded);
    assert_eq!(changes[0].severity, Severity::Breaking);
}

#[test]
fn added_optional_request_property_is_safe() {
    let changes = openapi_changes(
        &request_spec("{type: object, properties: {}}"),
        &request_spec("{type: object, properties: {name: {type: string}}}"),
    );
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].severity, Severity::Safe);
}

#[test]
fn added_required_response_property_is_safe() {
    let changes = openapi_changes(
        &response_spec("{type: object, properties: {}}"),
        &response_spec("{type: object, properties: {name: {type: string}}, required: [name]}"),
    );
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].severity, Severity::Safe);
}

#[test]
fn added_required_request_property_in_all_of_is_a_breaking_change() {
    let changes = openapi_changes(
        &request_spec("{allOf: [{type: object, properties: {}}]}"),
        &request_spec(
            "{allOf: [{type: object, properties: {name: {type: string}}, required: [name]}]}",
        ),
    );
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].severity, Severity::Breaking);
}

#[test]
fn request_nullability_narrowing_is_a_breaking_change() {
    let changes = openapi_changes(
        &request_spec("{type: string, nullable: true}"),
        &request_spec("{type: string}"),
    );
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].kind, ChangeKind::NullabilityChanged);
    assert_eq!(changes[0].severity, Severity::Breaking);
}

#[test]
fn request_nullability_widening_is_safe() {
    let changes = openapi_changes(
        &request_spec("{type: string}"),
        &request_spec("{type: string, nullable: true}"),
    );
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].severity, Severity::Safe);
}

#[test]
fn response_nullability_widening_is_a_breaking_change() {
    let changes = openapi_changes(
        &response_spec("{type: string}"),
        &response_spec("{type: string, nullable: true}"),
    );
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].kind, ChangeKind::NullabilityChanged);
    assert_eq!(changes[0].severity, Severity::Breaking);
}

#[test]
fn response_nullability_narrowing_is_safe() {
    let changes = openapi_changes(
        &response_spec("{type: string, nullable: true}"),
        &response_spec("{type: string}"),
    );
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].severity, Severity::Safe);
}

#[test]
fn root_integer_minimum_tightening_is_detected() {
    let changes = openapi_changes(
        &request_spec("{type: integer, minimum: 0}"),
        &request_spec("{type: integer, minimum: 10}"),
    );
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].kind, ChangeKind::ConstraintChanged);
    assert_eq!(changes[0].severity, Severity::Breaking);
}

#[test]
fn root_string_format_change_is_detected() {
    let changes = openapi_changes(
        &response_spec("{type: string, format: date}"),
        &response_spec("{type: string, format: date-time}"),
    );
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].kind, ChangeKind::ConstraintChanged);
}

#[test]
fn root_string_max_length_tightening_is_detected() {
    let changes = openapi_changes(
        &request_spec("{type: string, maxLength: 100}"),
        &request_spec("{type: string, maxLength: 50}"),
    );
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].severity, Severity::Breaking);
}

#[test]
fn nested_scalar_array_item_minimum_is_detected() {
    let changes = openapi_changes(
        &request_spec("{type: array, items: {type: array, items: {type: integer, minimum: 0}}}"),
        &request_spec("{type: array, items: {type: array, items: {type: integer, minimum: 10}}}"),
    );
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].severity, Severity::Breaking);
}

#[test]
fn scalar_property_constraint_is_not_reported_twice() {
    let changes = openapi_changes(
        &request_spec("{type: object, properties: {count: {type: integer, minimum: 0}}}"),
        &request_spec("{type: object, properties: {count: {type: integer, minimum: 10}}}"),
    );
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].kind, ChangeKind::ConstraintChanged);
}

#[test]
fn non_null_graphql_argument_default_removal_is_a_breaking_change() {
    let changes = gql_changes(
        "type Query { items(limit: Int! = 10): String }",
        "type Query { items(limit: Int!): String }",
    );
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].path, "Query.items(limit:)");
    assert_eq!(changes[0].kind, ChangeKind::RequiredChanged);
    assert_eq!(changes[0].severity, Severity::Breaking);
}

#[test]
fn non_null_graphql_input_default_removal_is_a_breaking_change() {
    let changes = gql_changes(
        "input Filter { limit: Int! = 10 } type Query { items(filter: Filter): String }",
        "input Filter { limit: Int! } type Query { items(filter: Filter): String }",
    );
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].path, "Filter.limit");
    assert_eq!(changes[0].kind, ChangeKind::RequiredChanged);
    assert_eq!(changes[0].severity, Severity::Breaking);
}

#[test]
fn nullable_graphql_argument_default_removal_does_not_make_it_required() {
    let changes = gql_changes(
        "type Query { items(limit: Int = 10): String }",
        "type Query { items(limit: Int): String }",
    );
    assert!(changes.is_empty());
}

#[test]
fn nullable_graphql_input_default_removal_does_not_make_it_required() {
    let changes = gql_changes(
        "input Filter { limit: Int = 10 } type Query { items: String }",
        "input Filter { limit: Int } type Query { items: String }",
    );
    assert!(changes.is_empty());
}

#[test]
fn graphql_default_addition_does_not_make_argument_required() {
    let changes = gql_changes(
        "type Query { items(limit: Int!): String }",
        "type Query { items(limit: Int! = 10): String }",
    );
    assert!(changes.is_empty());
}
