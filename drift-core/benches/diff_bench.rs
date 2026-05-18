// D-6: Performance benchmarks — parse+diff must stay well under the p95 targets
// (check p95 < 5 s, blast-radius p95 < 300 ms).
use criterion::{criterion_group, criterion_main, Criterion};
use drift_core::diff::{diff_openapi, parse_openapi};
use drift_core::graphql::{diff_graphql, parse_graphql};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const SMALL_OPENAPI: &str = r#"
openapi: "3.0.0"
info:
  title: Test API
  version: "1.0"
paths:
  /users:
    get:
      operationId: listUsers
      responses:
        "200":
          description: ok
          content:
            application/json:
              schema:
                type: object
                properties:
                  id:
                    type: string
                  name:
                    type: string
                  email:
                    type: string
  /users/{id}:
    get:
      operationId: getUser
      parameters:
        - name: id
          in: path
          required: true
          schema:
            type: string
      responses:
        "200":
          description: ok
"#;

const SMALL_OPENAPI_MODIFIED: &str = r#"
openapi: "3.0.0"
info:
  title: Test API
  version: "2.0"
paths:
  /users:
    get:
      operationId: listUsers
      responses:
        "200":
          description: ok
          content:
            application/json:
              schema:
                type: object
                properties:
                  id:
                    type: string
                  name:
                    type: string
  /users/{id}:
    get:
      operationId: getUser
      parameters:
        - name: id
          in: path
          required: true
          schema:
            type: string
      responses:
        "200":
          description: ok
  /posts:
    get:
      operationId: listPosts
      responses:
        "200":
          description: ok
"#;

const SMALL_GRAPHQL: &str = r#"
type User {
  id: ID!
  name: String!
  email: String
  role: Role!
}

enum Role {
  ADMIN
  VIEWER
  EDITOR
}

type Query {
  user(id: ID!): User
  users: [User!]!
}
"#;

const SMALL_GRAPHQL_MODIFIED: &str = r#"
type User {
  id: ID!
  name: String!
  role: Role!
}

enum Role {
  ADMIN
  VIEWER
}

type Query {
  user(id: ID!): User
  users: [User!]!
}
"#;

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn bench_openapi_parse(c: &mut Criterion) {
    c.bench_function("openapi_parse_small", |b| {
        b.iter(|| parse_openapi(SMALL_OPENAPI).expect("parse failed"))
    });
}

fn bench_openapi_diff(c: &mut Criterion) {
    let base = parse_openapi(SMALL_OPENAPI).unwrap();
    let head = parse_openapi(SMALL_OPENAPI_MODIFIED).unwrap();
    c.bench_function("openapi_diff_small", |b| {
        b.iter(|| diff_openapi(&base, &head))
    });
}

fn bench_openapi_parse_and_diff(c: &mut Criterion) {
    c.bench_function("openapi_parse_and_diff_small", |b| {
        b.iter(|| {
            let base = parse_openapi(SMALL_OPENAPI).unwrap();
            let head = parse_openapi(SMALL_OPENAPI_MODIFIED).unwrap();
            diff_openapi(&base, &head)
        })
    });
}

fn bench_graphql_parse(c: &mut Criterion) {
    c.bench_function("graphql_parse_small", |b| {
        b.iter(|| parse_graphql(SMALL_GRAPHQL).expect("parse failed"))
    });
}

fn bench_graphql_diff(c: &mut Criterion) {
    let base = parse_graphql(SMALL_GRAPHQL).unwrap();
    let head = parse_graphql(SMALL_GRAPHQL_MODIFIED).unwrap();
    c.bench_function("graphql_diff_small", |b| {
        b.iter(|| diff_graphql(&base, &head))
    });
}

criterion_group!(
    benches,
    bench_openapi_parse,
    bench_openapi_diff,
    bench_openapi_parse_and_diff,
    bench_graphql_parse,
    bench_graphql_diff,
);
criterion_main!(benches);
