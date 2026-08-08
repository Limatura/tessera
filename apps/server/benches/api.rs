//! Benchmark skeleton.
//!
//! Deliberately thin for now: it exists so that the harness, the profile and the
//! CI entry point are already in place when there is something worth measuring
//! (retrieval-code lookup, streaming upload throughput, expiry sweeps).
//!
//! Run with `cargo bench`; results land in `target/criterion`.
//!
//! `clippy.toml`'s `allow-expect-in-tests` only reaches `#[test]` function bodies,
//! so the same exemption has to be restated for benchmark setup. A benchmark that
//! cannot build its fixtures has no result to report.
#![allow(
    clippy::expect_used,
    reason = "benchmark fixture setup; see module docs"
)]

use std::{hint::black_box, sync::Arc};

use criterion::{Criterion, criterion_group, criterion_main};
use tessera_server::{
    api,
    config::{Config, DatabaseConfig, StorageConfig},
    db,
    state::{AppState, ServiceStatus},
    storage,
};

/// Measures the fixed cost of turning handlers into an OpenAPI document. This is
/// start-up work, not request work, but it is the one thing already worth
/// watching: the document is rebuilt on every CI run and every contract check.
fn openapi_generation(c: &mut Criterion) {
    c.bench_function("openapi::build_document", |b| {
        b.iter(|| black_box(api::openapi()));
    });
}

/// Measures router assembly, which every process start pays once.
fn router_assembly(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("benchmark runtime");

    let temp = tempfile::tempdir().expect("temp dir");
    let config = Config {
        database: DatabaseConfig {
            url: "sqlite::memory:".to_owned(),
            ..Default::default()
        },
        storage: StorageConfig {
            local_path: temp.path().join("blobs"),
            ..Default::default()
        },
        ..Config::default()
    };

    let db = runtime
        .block_on(db::connect(&config.database))
        .expect("sqlite pool");
    let store = storage::build(&config.storage).expect("local store");
    let state = AppState::new(Arc::new(config), db, store);
    state.set_status(ServiceStatus::Ready);

    c.bench_function("api::build_router", |b| {
        b.iter(|| black_box(api::build(state.clone())));
    });
}

criterion_group!(benches, openapi_generation, router_assembly);
criterion_main!(benches);
