//! The HTTP API, as much of it as the commands use.

use reqwest::{RequestBuilder, Response, StatusCode};
use serde::{Deserialize, Serialize};
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Upload {
    /// Where to continue it.
    pub id: String,
    /// How much to send per request.
    pub part_size: u64,
    /// What an anonymous uploader continues it with.
    pub token: Option<String>,
}

/// A sign-in waiting for a person to approve it in a browser.
#[derive(Debug, Clone, Deserialize)]
pub struct DeviceSignIn {
    /// What this program polls with. Never shown.
    pub device_code: String,
    /// What the person enters on the site.
    pub user_code: String,
    /// Seconds until both codes stop working.
    pub expires_in: u64,
    /// Seconds to wait between polls.
    pub interval: u64,
}

#[derive(Deserialize)]
struct Issued {
    secret: String,
}

/// One of the account's own shares, as `tess ls` shows it.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct OwnShare {
    /// The retrieval code.
    pub code: String,
    /// What the uploader called it, if anything.
    pub title: Option<String>,
    /// The files, in order. Empty for a note.
    #[serde(default)]
    pub files: Vec<OwnFile>,
    /// `link` or `password` when sealed on the uploader's machine.
    pub encryption: Option<String>,
    /// When it stops being served, RFC 3339, or `None` for never.
    pub expires_at: Option<String>,
    /// Retrievals left, or `None` for unlimited.
    pub uses_remaining: Option<u64>,
    /// `file`, `text` or `link`.
    #[serde(default)]
    pub kind: Option<String>,
    /// A note's language, as the site names them.
    #[serde(default)]
    pub language: Option<String>,
    /// When it was made, RFC 3339.
    #[serde(default)]
    pub created_at: Option<String>,
    /// Bytes stored for it.
    #[serde(default)]
    pub size_bytes: Option<u64>,
    /// How many times it has been retrieved.
    #[serde(default)]
    pub use_count: Option<u64>,
    /// Whether it asks for a password.
    #[serde(default)]
    pub has_password: bool,
}

/// One file of an own share.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct OwnFile {
    /// Its name, unless the share is encrypted.
    pub filename: Option<String>,
}

/// One page of the account's shares.
#[derive(Debug, Clone, Deserialize)]
pub struct SharePage {
    /// The shares on it, newest first.
    pub data: Vec<OwnShare>,
    /// How many there are in all.
    pub meta: PageMeta,
}

/// Where a page sits in the whole list.
#[derive(Debug, Clone, Deserialize)]
pub struct PageMeta {
    /// How many there are in all.
    pub total: u64,
}

/// An account, as much of it as the commands read.
#[derive(Debug, Clone, Deserialize)]
pub struct Account {
    /// Its name.
    pub username: String,
    /// What it may do, as `resource:verb` strings.
    pub permissions: Vec<String>,
}

/// Where a password sign-in got to.
#[derive(Debug, Clone)]
pub enum PasswordSignIn {
    /// Signed in. The session cookie, as `name=value`, and the account.
    Done {
        /// Sent back as the `Cookie` header.
        cookie: String,
        /// Who signed in.
        account: Account,
    },
    /// The password was right and the account asks for a code from its
    /// authenticator app. Finish with [`Client::finish_password_sign_in`].
    NeedsCode {
        /// What the code is sent back with.
        ticket: String,
    },
}

#[derive(Deserialize)]
struct Challenge {
    ticket: String,
}

/// The session cookie a sign-in set, as `name=value`.
fn session_cookie(response: &Response) -> Result<String> {
    response
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .filter_map(|value| value.split(';').next())
        .find(|pair| pair.starts_with(SESSION_COOKIE))
        .map(str::to_owned)
        .ok_or_else(|| Error::Protocol("signed in, but no session cookie came back".to_owned()))
}

/// The cookie the server keeps a browser session in.
const SESSION_COOKIE: &str = "tessera_session=";

#[derive(Deserialize)]
struct Ticket {
    ticket: String,
}

#[derive(Deserialize)]
struct Refusal {
    code: Option<String>,
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

    /// Starts signing in from a browser, as `client_name`.
    ///
    /// # Errors
    ///
    /// When the server refuses it, which an instance without accounts does.
    pub async fn start_device_sign_in(&self, client_name: &str) -> Result<DeviceSignIn> {
        let request = self
            .request(reqwest::Method::POST, self.url("/api/v1/auth/device")?)
            .json(&serde_json::json!({ "client_name": client_name }));
        Ok(accepted(request.send().await?).await?.json().await?)
    }

    /// Asks whether a sign-in started with `device_code` has been approved,
    /// and collects its token if it has.
    ///
    /// # Errors
    ///
    /// When the server refuses, which includes "not yet": that is
    /// `device_authorization_pending` in [`Error::Server`]'s `code`.
    pub async fn redeem_device_code(&self, device_code: &str) -> Result<String> {
        let request = self
            .request(
                reqwest::Method::POST,
                self.url("/api/v1/auth/device/token")?,
            )
            .json(&serde_json::json!({ "device_code": device_code }));
        let issued: Issued = accepted(request.send().await?).await?.json().await?;
        Ok(issued.secret)
    }

    /// The account this client's token belongs to.
    ///
    /// # Errors
    ///
    /// When there is no token, or the server does not accept it.
    pub async fn account(&self) -> Result<Account> {
        let request = self.request(reqwest::Method::GET, self.url("/api/v1/auth/session")?);
        Ok(accepted(request.send().await?).await?.json().await?)
    }

    /// Signs in with a username or address and a password.
    ///
    /// # Errors
    ///
    /// When the server refuses, which a wrong password does.
    pub async fn sign_in_with_password(
        &self,
        identifier: &str,
        password: &str,
    ) -> Result<PasswordSignIn> {
        let request = self
            .http
            .post(self.url("/api/v1/auth/login")?)
            .json(&serde_json::json!({ "identifier": identifier, "password": password }));
        let response = accepted(request.send().await?).await?;
        if response.status() == StatusCode::ACCEPTED {
            let challenge: Challenge = response.json().await?;
            return Ok(PasswordSignIn::NeedsCode {
                ticket: challenge.ticket,
            });
        }
        let cookie = session_cookie(&response)?;
        Ok(PasswordSignIn::Done {
            cookie,
            account: response.json().await?,
        })
    }

    /// Finishes a password sign-in with a code from the authenticator app, or
    /// a recovery code.
    ///
    /// # Errors
    ///
    /// When the code is wrong or the ticket has run out.
    pub async fn finish_password_sign_in(
        &self,
        ticket: &str,
        code: &str,
    ) -> Result<PasswordSignIn> {
        let request = self
            .http
            .post(self.url("/api/v1/auth/login/mfa")?)
            .json(&serde_json::json!({ "ticket": ticket, "code": code }));
        let response = accepted(request.send().await?).await?;
        let cookie = session_cookie(&response)?;
        Ok(PasswordSignIn::Done {
            cookie,
            account: response.json().await?,
        })
    }

    /// Issues an API token named `name` with `permissions`, from a session.
    ///
    /// # Errors
    ///
    /// When the server refuses it.
    pub async fn create_token(
        &self,
        cookie: &str,
        name: &str,
        permissions: &[&str],
    ) -> Result<String> {
        let request = self
            .http
            .post(self.url("/api/v1/tokens")?)
            .header(reqwest::header::COOKIE, cookie)
            .json(&serde_json::json!({ "name": name, "permissions": permissions }));
        let issued: Issued = accepted(request.send().await?).await?.json().await?;
        Ok(issued.secret)
    }

    /// Ends a session.
    ///
    /// # Errors
    ///
    /// When the request does not get an answer.
    pub async fn sign_out(&self, cookie: &str) -> Result<()> {
        let request = self
            .http
            .post(self.url("/api/v1/auth/logout")?)
            .header(reqwest::header::COOKIE, cookie);
        accepted(request.send().await?).await?;
        Ok(())
    }

    /// One page of the signed-in account's shares, `offset` in.
    ///
    /// # Errors
    ///
    /// When there is no token, or the server refuses.
    pub async fn list_shares(&self, offset: u64) -> Result<SharePage> {
        let mut url = self.url("/api/v1/shares")?;
        url.query_pairs_mut()
            .append_pair("offset", &offset.to_string());
        let request = self.request(reqwest::Method::GET, url);
        Ok(accepted(request.send().await?).await?.json().await?)
    }

    /// Deletes one of the signed-in account's shares.
    ///
    /// # Errors
    ///
    /// When there is no such share of this account's, or the server refuses.
    pub async fn delete_share(&self, code: &str) -> Result<()> {
        let request = self.request(
            reqwest::Method::DELETE,
            self.url(&format!("/api/v1/shares/{code}"))?,
        );
        accepted(request.send().await?).await?;
        Ok(())
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

/// What an instance allows and how it looks, as much as the full-screen view reads.
#[derive(Debug, Clone, Deserialize)]
pub struct Capabilities {
    /// Whether shares may be sealed end to end here.
    pub encryption: bool,
    /// Whether someone not signed in may upload.
    pub anonymous_upload: bool,
    /// How long a share lasts when not told, in hours; `None` for no limit.
    pub default_expiry_hours: Option<u64>,
    /// The longest a share may last, in hours; `None` for no limit.
    pub max_expiry_hours: Option<u64>,
    /// The site's look. Missing from servers before 0.3.
    #[serde(default)]
    pub appearance: Appearance,
}

/// A look, as names. Only the accent is read: a terminal keeps its own
/// background, so the site's neutral palette and light or dark have no say.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Appearance {
    /// `violet`, `rose`, `orange`, `amber`, `green`, `teal` or `blue`.
    pub accent: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Preferences {
    #[serde(default)]
    appearance: Appearance,
}

impl Client {
    /// What this instance allows, and its look. Asked of anyone.
    ///
    /// # Errors
    ///
    /// When the request fails.
    pub async fn capabilities(&self) -> Result<Capabilities> {
        let request = self.request(reqwest::Method::GET, self.url("/api/v1/capabilities")?);
        Ok(accepted(request.send().await?).await?.json().await?)
    }

    /// The accent the signed-in account picked on the site, if it picked one.
    ///
    /// # Errors
    ///
    /// When there is no token, or the request fails.
    pub async fn chosen_accent(&self) -> Result<Option<String>> {
        let request = self.request(reqwest::Method::GET, self.url("/api/v1/me/preferences")?);
        let preferences: Preferences = accepted(request.send().await?).await?.json().await?;
        Ok(preferences.appearance.accent)
    }
}

/// The response, or the refusal it carries.
async fn accepted(response: Response) -> Result<Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let (code, message) = response.json::<Refusal>().await.map_or_else(
        |_| {
            (
                None,
                status
                    .canonical_reason()
                    .unwrap_or("request refused")
                    .to_owned(),
            )
        },
        |refusal| (refusal.code, refusal.message),
    );
    Err(Error::Server {
        status: status.as_u16(),
        code,
        message,
    })
}
