//! The shares tables, exercised through the repository.
//!
//! The scenario itself lives in [`common::share_repository_round_trip`] because
//! the cross-backend suite runs the same one — the per-dialect SQL is
//! hand-written, so only writing rows and reading them back proves the entity
//! and the schema agree.

mod common;

use common::{TestApp, share_repository_round_trip};

#[tokio::test]
async fn shares_round_trip_through_the_repository() {
    share_repository_round_trip(&TestApp::ready().await, "SQ").await;
}
