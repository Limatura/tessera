//! Extractors that fail the way the rest of the API does.
//!
//! # Why these exist
//!
//! `axum`'s own `Json`, `Query` and `Path` reject with plain text and status
//! codes of their own choosing: `400` for malformed JSON, `422` for a missing
//! field, `415` for the wrong content type. That breaks the one promise the
//! contract makes about failures, that **every** non-2xx response is an
//! [`ErrorBody`](crate::error::ErrorBody) a client can branch on. A front-end
//! switching on `error.code` gets `undefined` the first time a malformed
//! request arrives, and `422` is not even in [`ErrorCode`].
//!
//! These wrap the originals and route the rejection through [`Error`], so the
//! shape holds everywhere. The rejection's own text is kept, because it
//! describes what the caller sent ("missing field `password`"), the
//! kind of detail an error's `message` may echo and far more useful than a
//! generic message.
//!
//! Every rejection becomes `400 bad_request`. Collapsing `415` and `422` into
//! it loses a distinction nothing acts on, since the client's next move is "fix
//! the request" either way, and keeps [`ErrorCode`] closed instead of growing
//! a variant per framework detail.
//!
//! [`ErrorCode`]: crate::error::ErrorCode

use axum::{
    extract::{
        FromRequest, FromRequestParts, Request,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::request::Parts,
};
use serde::de::DeserializeOwned;

use crate::error::{Error, ErrorCode};

/// [`axum::Json`], rejecting with an [`ErrorBody`](crate::error::ErrorBody).
#[derive(Debug, Clone, Copy, Default)]
pub struct ApiJson<T>(pub T);

/// [`axum::extract::Query`], rejecting with an
/// [`ErrorBody`](crate::error::ErrorBody).
#[derive(Debug, Clone, Copy, Default)]
pub struct ApiQuery<T>(pub T);

/// [`axum::extract::Path`], rejecting with an
/// [`ErrorBody`](crate::error::ErrorBody).
#[derive(Debug, Clone, Copy, Default)]
pub struct ApiPath<T>(pub T);

impl<T, S> FromRequest<S> for ApiJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let axum::Json(value) = axum::Json::<T>::from_request(request, state)
            .await
            .map_err(|rejection| describe_json(&rejection))?;
        Ok(Self(value))
    }
}

impl<T, S> FromRequestParts<S> for ApiQuery<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let axum::extract::Query(value) =
            axum::extract::Query::<T>::from_request_parts(parts, state)
                .await
                .map_err(|rejection| describe_query(&rejection))?;
        Ok(Self(value))
    }
}

impl<T, S> FromRequestParts<S> for ApiPath<T>
where
    T: DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let axum::extract::Path(value) = axum::extract::Path::<T>::from_request_parts(parts, state)
            .await
            .map_err(|rejection| describe_path(&rejection))?;
        Ok(Self(value))
    }
}

fn describe_json(rejection: &JsonRejection) -> Error {
    Error::new(ErrorCode::InvalidBody).with_message(rejection.body_text())
}

fn describe_query(rejection: &QueryRejection) -> Error {
    Error::new(ErrorCode::InvalidQuery).with_message(rejection.body_text())
}

fn describe_path(rejection: &PathRejection) -> Error {
    Error::new(ErrorCode::InvalidPath).with_message(rejection.body_text())
}
