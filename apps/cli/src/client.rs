//! The HTTP API, as much of it as the commands use.

use reqwest::{RequestBuilder, Response, StatusCode};
use serde::Deserialize;
use url::Url;

use crate::{Error, Result};

/// A connection to one instance.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::Client,
    server: Url,
    token: Option<String>,
}

/// A share as its uploader sees it. Only the fields the commands read.
#[derive(Debug, Clone, Deserialize)]
pub struct Share {
    /// The retrieval code.
    pub code: String,
    /// Shown once, for an anonymous upload: what lets it be changed later.
    pub management_token: Option<String>,
}

/// A share as anyone holding its code sees it.
#[derive(Debug, Clone, Deserialize)]
pub struct Described {
    /// `file` or `text`.
    pub kind: String,
    /// Whether a password stands in front of it.
    pub has_password: bool,
    /// `link`, `password`, or `None` for a share the server can read.
    pub encryption: Option<String>,
    /// The salt a password-derived key is stretched with.
    pub key_salt: Option<String>,
    /// Whether the fields below are filled in.
    pub unlocked: bool,
    /// The files, in order.
    #[serde(default)]
    pub files: Vec<DescribedFile>,
}

/// One file of a described share.
#[derive(Debug, Clone, Deserialize)]
pub struct DescribedFile {
    /// Its name, on a share the server can read.
    pub filename: Option<String>,
    /// Its name and type, sealed, on an encrypted share.
    pub sealed_name: Option<String>,
    /// How many bytes a download sends.
    pub size_bytes: u64,
}

/// A session for sending one file.
#[derive(Debug, Clone, Deserialize)]
pub struct Upload {
    /// Where to continue it.
    pub id: String,
    /// How much to send per request.
    pub part_size: u64,
    /// What an anonymous uploader continues it with.
    pub token: Option<String>,
}

#[derive(Deserialize)]
struct Ticket {
    ticket: String,
}

#[derive(Deserialize)]
struct Refusal {
    message: String,
}

impl Client {
    /// A client for the instance at `server`, signed in with an API token or
    /// anonymous.
    ///
    /// # Errors
    ///
    /// When the HTTP client cannot be built, which means no TLS roots.
    pub fn new(server: Url, token: Option<String>) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("tessera-cli/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            http,
            server,
            token,
        })
    }

    /// The instance this talks to.
    #[must_use]
    pub const fn server(&self) -> &Url {
        &self.server
    }

    /// Whether requests carry an account's token.
    #[must_use]
    pub const fn signed_in(&self) -> bool {
        self.token.is_some()
    }

    /// An absolute URL on the instance.
    ///
    /// # Errors
    ///
    /// For a path that does not join, which only a bug produces.
    pub fn url(&self, path: &str) -> Result<Url> {
        self.server
            .join(path)
            .map_err(|error| Error::Usage(format!("bad server address: {error}")))
    }

    /// A request, signed in when there is a token.
    pub(crate) fn request(&self, method: reqwest::Method, url: Url) -> RequestBuilder {
        let request = self.http.request(method, url);
        match &self.token {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }

    /// Opens an upload session for `length` bytes.
    ///
    /// # Errors
    ///
    /// When the server refuses it.
    pub async fn open_upload(
        &self,
        query: &[(&str, String)],
        length: u64,
        management: Option<&str>,
    ) -> Result<Upload> {
        let mut url = self.url("/api/v1/uploads")?;
        url.query_pairs_mut().extend_pairs(query);
        let mut request = self
            .request(reqwest::Method::POST, url)
            .header("upload-length", length);
        if let Some(token) = management {
            request = request.header("x-share-management-token", token);
        }
        Ok(accepted(request.send().await?).await?.json().await?)
    }

    /// Sends one part at `offset`. `Some` share once that was the last of it.
    ///
    /// # Errors
    ///
    /// When the request fails or the server refuses the part.
    pub async fn send_part(
        &self,
        upload: &Upload,
        offset: u64,
        part: Vec<u8>,
    ) -> Result<Option<Share>> {
        let mut request = self
            .request(
                reqwest::Method::PATCH,
                self.url(&format!("/api/v1/uploads/{}", upload.id))?,
            )
            .header("content-type", "application/offset+octet-stream")
            .header("upload-offset", offset)
            .body(part);
        if let Some(token) = &upload.token {
            request = request.header("x-upload-token", token);
        }
        let response = accepted(request.send().await?).await?;
        if response.status() == StatusCode::CREATED {
            return Ok(Some(response.json().await?));
        }
        Ok(None)
    }

    /// How much of an upload has arrived.
    ///
    /// # Errors
    ///
    /// When the session is gone or the answer has no offset.
    pub async fn upload_offset(&self, upload: &Upload) -> Result<u64> {
        let mut request = self.request(
            reqwest::Method::HEAD,
            self.url(&format!("/api/v1/uploads/{}", upload.id))?,
        );
        if let Some(token) = &upload.token {
            request = request.header("x-upload-token", token);
        }
        let response = accepted(request.send().await?).await?;
        response
            .headers()
            .get("upload-offset")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse().ok())
            .ok_or_else(|| Error::Protocol("no Upload-Offset on a HEAD".to_owned()))
    }

    /// Creates a note from `body`.
    ///
    /// # Errors
    ///
    /// When the server refuses it.
    pub async fn create_note(&self, query: &[(&str, String)], body: Vec<u8>) -> Result<Share> {
        let mut url = self.url("/api/v1/shares")?;
        url.query_pairs_mut().extend_pairs(query);
        let request = self.request(reqwest::Method::POST, url).body(body);
        Ok(accepted(request.send().await?).await?.json().await?)
    }

    /// Changes a share's settings once it exists.
    ///
    /// # Errors
    ///
    /// When the server refuses the change.
    pub async fn update_share(
        &self,
        code: &str,
        management: Option<&str>,
        password: Option<&str>,
        settings: &serde_json::Value,
    ) -> Result<()> {
        let mut request = self
            .request(
                reqwest::Method::PATCH,
                self.url(&format!("/api/v1/shares/{code}"))?,
            )
            .json(settings);
        if let Some(token) = management {
            request = request.header("x-share-management-token", token);
        }
        if let Some(password) = password {
            request = request.header("x-share-password", password);
        }
        accepted(request.send().await?).await?;
        Ok(())
    }

    /// Describes a share without spending a retrieval.
    ///
    /// # Errors
    ///
    /// When there is no such share, or the password is wrong.
    pub async fn describe(&self, code: &str, password: Option<&str>) -> Result<Described> {
        let mut request = self.request(
            reqwest::Method::GET,
            self.url(&format!("/api/v1/shares/{code}"))?,
        );
        if let Some(password) = password {
            request = request.header("x-share-password", password);
        }
        Ok(accepted(request.send().await?).await?.json().await?)
    }

    /// Spends one retrieval and returns the ticket every transfer of it uses.
    ///
    /// # Errors
    ///
    /// When the share is gone, used up, or the password is wrong.
    pub async fn unlock(&self, code: &str, password: Option<&str>) -> Result<String> {
        let mut request = self.request(
            reqwest::Method::POST,
            self.url(&format!("/api/v1/shares/{code}/unlock"))?,
        );
        if let Some(password) = password {
            request = request.header("x-share-password", password);
        }
        let ticket: Ticket = accepted(request.send().await?).await?.json().await?;
        Ok(ticket.ticket)
    }

    /// Starts a download from `from` bytes in. Follows the server to the store
    /// when it sends one there.
    ///
    /// # Errors
    ///
    /// When the server refuses it.
    pub async fn download(&self, url: Url, from: u64) -> Result<Response> {
        let mut request = self.request(reqwest::Method::GET, url);
        if from > 0 {
            request = request.header("range", format!("bytes={from}-"));
        }
        accepted(request.send().await?).await
    }
}

/// The response, or the refusal it carries.
async fn accepted(response: Response) -> Result<Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let message = response.json::<Refusal>().await.map_or_else(
        |_| {
            status
                .canonical_reason()
                .unwrap_or("request refused")
                .to_owned()
        },
        |refusal| refusal.message,
    );
    Err(Error::Server {
        status: status.as_u16(),
        message,
    })
}
