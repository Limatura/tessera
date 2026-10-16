//! Contract tests for the generated OpenAPI document.
//!
//! Two distinct guarantees:
//!
//! 1. The snapshot catches *accidental* contract changes. Review the diff and run `cargo insta
//!    accept` when the change was intended.
//! 2. The drift check catches a contract change that was intended but never written back to
//!    `openapi.json`, the file every client generates from. CI runs the same comparison.

use std::path::PathBuf;

/// `openapi.json` lives at the repository root, two levels above this crate.
fn committed_contract_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../openapi.json")
}

#[test]
fn openapi_document_matches_its_snapshot() {
    // `openapi_json()`, not `openapi()`. The repaired document is the one
    // that ships, and a snapshot of anything else would pass while the artefact
    // changed underneath it.
    let document = tessera_server::api::openapi_json();
    // The release version is not part of the contract; every release would
    // otherwise rewrite the snapshot.
    insta::assert_json_snapshot!("openapi", document, { ".info.version" => "[version]" });
}

#[test]
fn committed_openapi_json_is_up_to_date() {
    let path = committed_contract_path();
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "could not read {}: {error}\nregenerate it with `cargo run --bin tessera-openapi`",
            path.display()
        )
    });

    // Exactly what `tessera-openapi` writes, so that "up to date" means the file
    // and the binary agree instead of merely resembling each other.
    let generated = format!(
        "{}\n",
        serde_json::to_string_pretty(&tessera_server::api::openapi_json())
            .expect("serialise the contract")
    );

    assert_eq!(
        committed.replace("\r\n", "\n"),
        generated,
        "{} is stale — regenerate it with `cargo run --bin tessera-openapi`",
        path.display()
    );
}

#[test]
fn health_is_described_with_both_outcomes() {
    let document = tessera_server::api::openapi();
    let health = document
        .paths
        .paths
        .get("/health")
        .and_then(|item| item.get.as_ref())
        .expect("/health must be in the contract");

    assert!(health.responses.responses.contains_key("200"));
    assert!(health.responses.responses.contains_key("503"));
}
