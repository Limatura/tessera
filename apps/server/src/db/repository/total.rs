//! `SUM()`, in a form every backend hands back as a `BIGINT`.
//!
//! Postgres widens `SUM(bigint)` to `NUMERIC`, so decoding the result as an
//! `i64` fails at the driver with "mismatched types" — not at compile time, not
//! in the SQLite suite, only against a real Postgres and only once somebody set
//! a quota. Casting the sum back is what makes one query text work everywhere.
//!
//! The cast is spelled per dialect because there is no spelling all three take:
//! MySQL's `CAST` wants `SIGNED` and refuses `BIGINT`, Postgres wants `BIGINT`
//! and refuses `SIGNED`, and SQLite needs no cast at all.

use sea_orm::{
    DbBackend,
    sea_query::{Alias, Expr, ExprTrait as _, Func},
};

/// `SUM(expr)`, as an `i64` on every backend.
pub(crate) fn bytes(backend: DbBackend, expr: Expr) -> Expr {
    let summed = Expr::from(Func::sum(expr));
    match backend {
        DbBackend::Postgres => summed.cast_as(Alias::new("BIGINT")),
        DbBackend::MySql => summed.cast_as(Alias::new("SIGNED")),
        // SQLite needs no cast, and a backend this version has never seen is
        // likelier to behave like it than like Postgres — an unnecessary cast
        // is a syntax error, a missing one is a decode error, and only the
        // second says in its message what to do about it.
        _ => summed,
    }
}
