//! Query-placeholder portability (N-26).
//!
//! The codebase writes queries with `?` positional placeholders. sqlx's `Any`
//! driver accepts `?` on SQLite but does NOT translate it for PostgreSQL (by
//! design — `?` is a syntax error there). Both backends DO accept `$1, $2, …`.
//!
//! Rather than convert every query literal (impossible for the queries built at
//! runtime with a variable placeholder count, e.g. blast-radius), we rewrite the
//! FINAL query string at execution time when the process is connected to
//! PostgreSQL. On SQLite the rewrite is skipped (a borrow, no allocation), so the
//! `?` form is used unchanged.
//!
//! Call sites use the `q!` / `qs!` / `qa!` macros (thin wrappers over
//! `sqlx::query` / `query_scalar` / `query_as`) which route the SQL through
//! [`pg`]. A `&pg(sql)` temporary lives to the end of the enclosing statement, so
//! the borrow the query holds is always valid.

use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering};

/// True when the active pool is PostgreSQL. Set once at pool creation.
static IS_POSTGRES: AtomicBool = AtomicBool::new(false);

/// Record whether the connection URL targets PostgreSQL, so [`pg`] rewrites
/// placeholders for it. Call once when the pool is created (and in test setup).
pub(crate) fn set_backend_from_url(url: &str) {
    IS_POSTGRES.store(url.starts_with("postgres"), Ordering::Relaxed);
}

/// Rewrite `?` placeholders to `$1, $2, …` when on PostgreSQL; otherwise return
/// the SQL unchanged (SQLite accepts `?`). `?` inside single-quoted SQL string
/// literals is left alone.
pub(crate) fn pg(sql: &str) -> Cow<'_, str> {
    if IS_POSTGRES.load(Ordering::Relaxed) {
        Cow::Owned(rewrite_placeholders(sql))
    } else {
        Cow::Borrowed(sql)
    }
}

/// Replace each `?` placeholder with a positional `$N`, skipping `?` inside
/// single-quoted string literals (SQL escapes an inner quote as `''`, which the
/// toggle handles correctly).
fn rewrite_placeholders(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len() + 8);
    let mut n: u32 = 0;
    let mut in_string = false;
    for c in sql.chars() {
        match c {
            '\'' => {
                in_string = !in_string;
                out.push(c);
            }
            '?' if !in_string => {
                n += 1;
                out.push('$');
                out.push_str(&n.to_string());
            }
            _ => out.push(c),
        }
    }
    out
}

/// `sqlx::query` with `$N`/`?` placeholder portability. See [`pg`].
macro_rules! q {
    ($sql:expr $(,)?) => {
        sqlx::query(&$crate::db::pg($sql))
    };
}

/// `sqlx::query_scalar` with placeholder portability.
macro_rules! qs {
    ($sql:expr $(,)?) => {
        sqlx::query_scalar(&$crate::db::pg($sql))
    };
}

/// `sqlx::query_as` with placeholder portability.
macro_rules! qa {
    ($sql:expr $(,)?) => {
        sqlx::query_as(&$crate::db::pg($sql))
    };
}

#[cfg(test)]
mod tests {
    use super::rewrite_placeholders;

    #[test]
    fn rewrites_placeholders_sequentially() {
        assert_eq!(
            rewrite_placeholders("INSERT INTO t (a, b, c) VALUES (?, ?, ?)"),
            "INSERT INTO t (a, b, c) VALUES ($1, $2, $3)"
        );
        assert_eq!(
            rewrite_placeholders("SELECT * FROM t WHERE a = ? AND b = ?"),
            "SELECT * FROM t WHERE a = $1 AND b = $2"
        );
    }

    #[test]
    fn skips_question_marks_inside_string_literals() {
        // A literal '?' must not be renumbered; the real placeholder becomes $1.
        assert_eq!(
            rewrite_placeholders("SELECT * FROM t WHERE label = 'huh?' AND id = ?"),
            "SELECT * FROM t WHERE label = 'huh?' AND id = $1"
        );
        // Escaped inner quote ('') keeps the string state correct.
        assert_eq!(
            rewrite_placeholders("SELECT 'it''s ok?' , ? , ?"),
            "SELECT 'it''s ok?' , $1 , $2"
        );
    }

    #[test]
    fn no_placeholders_is_unchanged() {
        assert_eq!(rewrite_placeholders("SELECT 1 FROM t"), "SELECT 1 FROM t");
    }
}
