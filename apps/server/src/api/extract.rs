//! Extractors that fail the way the rest of the API does.
//!
//! # Why these exist
//!
//! `axum`'s own `Json`, `Query` and `Path` reject with plain text and status
//! codes of their own: `400` for malformed JSON, `422` for a missing field,
//! `415` for the wrong content type. That breaks the contract's one promise
//! about failures, that **every** non-2xx response is an
//! [`ErrorBody`](crate::error::ErrorBody) a client can branch on. A frontend
//! switching on `error.code` would get `undefined` on the first malformed
//! request, and `422` is not even an [`ErrorCode`].
//!
//! These wrap the originals and route the rejection through [`Error`], so the
//! shape holds everywhere. The rejection's own text is kept as the message,
//! since it describes what the caller sent ("missing field `password`"), which
//! is far more useful than a generic message.
//!
//! Each rejection becomes a `400` with `invalid_body`, `invalid_query` or
//! `invalid_path`. Folding `415` and `422` in loses a distinction nothing acts
//! on, since the client's next step is "fix the request" either way, and keeps
//! [`ErrorCode`] from growing a variant per framework detail.
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
        // Read as JSON first, which keeps axum's answers for a missing
        // content type and for a body that is not JSON at all.
        let axum::Json(value) = axum::Json::<serde_json::Value>::from_request(request, state)
            .await
            .map_err(|rejection| describe_json(&rejection))?;
        // Every body this API takes is an object. Checked here because serde
        // also builds a struct from an array, matching elements to fields in
        // order, so `[]` passed as a request whose fields are all optional:
        // a body the contract calls invalid, accepted as "change nothing".
        if !value.is_object() {
            return Err(Error::new(ErrorCode::InvalidBody)
                .with_message("the request body must be a JSON object"));
        }
        // The same wording axum gives, path included, so a client sees no
        // difference for a body that is an object of the wrong shape.
        serde_path_to_error::deserialize(value)
            .map(Self)
            .map_err(|error| {
                Error::new(ErrorCode::InvalidBody).with_message(format!(
                    "Failed to deserialize the JSON body into the target type: {error}"
                ))
            })
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
