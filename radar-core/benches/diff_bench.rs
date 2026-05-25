use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use radar_core::diff::{diff_openapi, parse_openapi};

const PAYMENTS_V1: &str = include_str!("../../fixtures/demo-payments-api/v1.yaml");
const PAYMENTS_V2: &str = include_str!("../../fixtures/demo-payments-api/v2.yaml");

const WIDE_SPEC: &str = r#"
openapi: "3.0.3"
info:
  title: Wide API
  version: "1.0"
paths:
  /users/{id}:
    get:
      operationId: getUser
      parameters:
        - name: id
          in: path
          required: true
          schema: { type: string }
      responses:
        "200":
          description: ok
          content:
            application/json:
              schema:
                type: object
                properties:
                  id:     { type: string }
                  name:   { type: string }
                  email:  { type: string }
                  phone:  { type: string }
                  role:   { type: string }
                  status: { type: string }
  /orders/{id}:
    get:
      operationId: getOrder
      parameters:
        - name: id
          in: path
          required: true
          schema: { type: string }
      responses:
        "200":
          description: ok
          content:
            application/json:
              schema:
                type: object
                properties:
                  id:     { type: string }
                  total:  { type: number }
                  items:  { type: array, items: { type: string } }
"#;

const WIDE_SPEC_BREAKING: &str = r#"
openapi: "3.0.3"
info:
  title: Wide API
  version: "2.0"
paths:
  /users/{id}:
    get:
      operationId: getUser
      parameters:
        - name: id
          in: path
          required: true
          schema: { type: string }
      responses:
        "200":
          description: ok
          content:
            application/json:
              schema:
                type: object
                properties:
                  id:     { type: string }
                  name:   { type: string }
                  email:  { type: string }
                  role:   { type: string }
  /orders/{id}:
    get:
      operationId: getOrder
      parameters:
        - name: id
          in: path
          required: true
          schema: { type: string }
      responses:
        "200":
          description: ok
          content:
            application/json:
              schema:
                type: object
                properties:
                  id:     { type: string }
                  total:  { type: number }
"#;

fn bench_parse(c: &mut Criterion) {
    c.bench_function("parse_openapi/payments_v1", |b| {
        b.iter(|| parse_openapi(PAYMENTS_V1).unwrap())
    });
    c.bench_function("parse_openapi/wide_spec", |b| {
        b.iter(|| parse_openapi(WIDE_SPEC).unwrap())
    });
}

fn bench_diff(c: &mut Criterion) {
    let v1 = parse_openapi(PAYMENTS_V1).unwrap();
    let v2 = parse_openapi(PAYMENTS_V2).unwrap();
    let wide_base = parse_openapi(WIDE_SPEC).unwrap();
    let wide_head = parse_openapi(WIDE_SPEC_BREAKING).unwrap();

    let mut group = c.benchmark_group("diff_openapi");

    group.bench_function("payments_v1_v2", |b| {
        b.iter(|| diff_openapi(&v1, &v2))
    });

    group.bench_with_input(
        BenchmarkId::new("wide_spec", "2_fields_removed"),
        &(&wide_base, &wide_head),
        |b, (base, head)| b.iter(|| diff_openapi(base, head)),
    );

    group.bench_function("identical_spec", |b| {
        b.iter(|| diff_openapi(&v1, &v1))
    });

    group.finish();
}

criterion_group!(benches, bench_parse, bench_diff);
criterion_main!(benches);
