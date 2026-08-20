//! Uploads that survive a dropped connection.
//!
//! A file arrives across several requests instead of one, so a transfer that
//! dies at ninety percent resumes from ninety percent. The protocol is tus's
//! offset model — see `api::uploads` — and where the bytes go while the upload
//! is unfinished is [`sink`].

pub mod sink;
