//! Checking what a browser sends back from a passkey: the relying party's half
//! of WebAuthn, for passkeys only.
//!
//! # Why this is written here and not taken from a crate
//!
//! The crate most servers use links OpenSSL, which this binary refuses (see
//! `deny.toml`), and the ones that do not are either a pre-release or carry a
//! second implementation of every signature algorithm. What a passkey needs is
//! small: the checks in §7.1 and §7.2 of the specification, three signature
//! algorithms `aws-lc-rs` already has, and a CBOR reader for the key.
//!
//! # What "passkeys only" leaves out
//!
//! Attestation: proving which make and model of authenticator made a key, from
//! a certificate chain. It matters to an organisation that issues hardware and
//! wants nothing else used; this instance accepts whatever authenticator the
//! person has, so the statement is read past and not checked, which is what
//! asking for `attestation: "none"` means. What is stored, the credential id
//! and its COSE key, is the same whatever checks the registration, so adding
//! attestation later would not touch a passkey already registered.
//!
//! # The algorithms
//!
//! ES256, EdDSA and RS256: between them every platform authenticator (Apple,
//! Google, Windows Hello) and the hardware keys people own.

use aws_lc_rs::signature::{
    ECDSA_P256_SHA256_ASN1, ED25519, RSA_PKCS1_2048_8192_SHA256, RsaPublicKeyComponents,
    UnparsedPublicKey,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use ciborium::Value;
use serde::Deserialize;
use sha2::{Digest as _, Sha256};

/// COSE algorithm identifiers this accepts, in the order a client should
/// prefer them.
pub const ALGORITHMS: [i64; 3] = [ES256, EDDSA, RS256];

const ES256: i64 = -7;
const EDDSA: i64 = -8;
const RS256: i64 = -257;

/// The user was present: touched the key, or looked at the camera.
const FLAG_USER_PRESENT: u8 = 0x01;
/// The user was verified: a PIN, a fingerprint, a face. What makes a passkey
/// stand in for a password and a second step together.
const FLAG_USER_VERIFIED: u8 = 0x04;
/// The credential may be synced to the person's other devices.
const FLAG_BACKUP_ELIGIBLE: u8 = 0x08;
/// It has been.
const FLAG_BACKED_UP: u8 = 0x10;
/// Attested credential data follows the counter: present when registering.
const FLAG_ATTESTED: u8 = 0x40;

/// Longest credential id the specification allows.
const MAX_CREDENTIAL_ID: usize = 1023;

/// Why a passkey response was not accepted.
///
/// Logged, never shown: the client gets one answer for all of them, so a
/// response cannot be refined against the server one check at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Rejection {
    /// A field is not the encoding it should be.
    #[error("a field is not well-formed")]
    Malformed,
    /// `clientDataJSON` is for a different ceremony.
    #[error("the client data is for the other ceremony")]
    WrongType,
    /// The page that asked is not this instance.
    #[error("the origin is not this instance")]
    WrongOrigin,
    /// The credential is for another site.
    #[error("the credential is scoped to another relying party")]
    WrongRelyingParty,
    /// The authenticator did not verify the person.
    #[error("the user was not present and verified")]
    NotVerified,
    /// A key type or algorithm this does not accept.
    #[error("the key's algorithm is not one this accepts")]
    UnsupportedKey,
    /// The signature does not verify.
    #[error("the signature does not verify")]
    BadSignature,
}

/// The site a passkey belongs to, and the page it may be used from.
#[derive(Debug, Clone)]
pub struct RelyingParty {
    /// The host name: what the credential is scoped to.
    pub id: String,
    /// `scheme://host[:port]`, exactly as a browser reports it.
    pub origin: String,
}

impl RelyingParty {
    /// Derives both from this instance's origin.
    ///
    /// `None` when the origin has no host, which no browser could be on.
    #[must_use]
    pub fn from_origin(origin: &str) -> Option<Self> {
        let parsed = url::Url::parse(origin).ok()?;
        let id = parsed.host_str()?.to_owned();
        Some(Self {
            id,
            origin: parsed.origin().ascii_serialization(),
        })
    }
}

/// A passkey that has passed registration, ready to be stored.
#[derive(Debug, Clone)]
pub struct NewCredential {
    /// The authenticator's identifier for it.
    pub id: Vec<u8>,
    /// Its public key, as the COSE structure the authenticator sent.
    pub public_key: Vec<u8>,
    /// The signature counter at registration.
    pub sign_count: u32,
    /// Whether it may sync to the person's other devices.
    pub backup_eligible: bool,
    /// Whether it has.
    pub backed_up: bool,
}

/// What a successful sign-in reports about the authenticator.
#[derive(Debug, Clone, Copy)]
pub struct Assertion {
    /// The signature counter now.
    pub sign_count: u32,
    /// Whether it has synced, which can change after registration.
    pub backed_up: bool,
}

/// The parts of `clientDataJSON` this checks.
#[derive(Deserialize)]
struct ClientData {
    #[serde(rename = "type")]
    kind: String,
    challenge: String,
    origin: String,
    #[serde(rename = "crossOrigin", default)]
    cross_origin: bool,
}

/// A fresh challenge: 32 random bytes, as base64url.
#[must_use]
pub fn new_challenge() -> String {
    let mut bytes = [0_u8; 32];
    crate::auth::secret::fill_random(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Decodes a base64url field from the browser.
///
/// # Errors
///
/// [`Rejection::Malformed`] when it is not base64url.
pub fn decode(field: &str) -> Result<Vec<u8>, Rejection> {
    // Some clients pad; the specification's JSON forms do not. Either is read.
    URL_SAFE_NO_PAD
        .decode(field.trim_end_matches('='))
        .map_err(|_| Rejection::Malformed)
}

/// Encodes bytes the way they are sent to the browser and stored.
#[must_use]
pub fn encode(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// The challenge a response answers, so the caller can find what it issued.
///
/// Read before anything is verified, and verifying then checks every other
/// field of the same document.
///
/// # Errors
///
/// [`Rejection::Malformed`] when `clientDataJSON` is not the JSON it should be.
pub fn challenge_of(client_data_json: &[u8]) -> Result<String, Rejection> {
    Ok(parse_client_data(client_data_json)?.challenge)
}

fn parse_client_data(raw: &[u8]) -> Result<ClientData, Rejection> {
    serde_json::from_slice(raw).map_err(|_| Rejection::Malformed)
}

fn check_client_data(raw: &[u8], kind: &str, rp: &RelyingParty) -> Result<(), Rejection> {
    let client = parse_client_data(raw)?;
    if client.kind != kind {
        return Err(Rejection::WrongType);
    }
    // An exact comparison. A browser reports the origin of the page, and a
    // page on any other origin, including a subdomain, is not this instance.
    if client.origin != rp.origin || client.cross_origin {
        return Err(Rejection::WrongOrigin);
    }
    Ok(())
}

/// The fixed-layout head of authenticator data.
struct AuthenticatorData<'a> {
    rp_id_hash: &'a [u8; 32],
    flags: u8,
    sign_count: u32,
    rest: &'a [u8],
}

fn parse_authenticator_data(bytes: &[u8]) -> Result<AuthenticatorData<'_>, Rejection> {
    let (rp_id_hash, rest) = bytes
        .split_first_chunk::<32>()
        .ok_or(Rejection::Malformed)?;
    let (&flags, rest) = rest.split_first().ok_or(Rejection::Malformed)?;
    let (counter, rest) = rest.split_first_chunk::<4>().ok_or(Rejection::Malformed)?;
    Ok(AuthenticatorData {
        rp_id_hash,
        flags,
        sign_count: u32::from_be_bytes(*counter),
        rest,
    })
}

/// The checks registration and sign-in share: the site, and a person who was
/// there and verified.
fn check_authenticator_data(
    data: &AuthenticatorData<'_>,
    rp: &RelyingParty,
) -> Result<(), Rejection> {
    let expected: [u8; 32] = Sha256::digest(rp.id.as_bytes()).into();
    if data.rp_id_hash != &expected {
        return Err(Rejection::WrongRelyingParty);
    }
    let required = FLAG_USER_PRESENT | FLAG_USER_VERIFIED;
    if data.flags & required != required {
        return Err(Rejection::NotVerified);
    }
    Ok(())
}

/// Checks a registration, §7.1, for a challenge the caller has already matched.
///
/// # Errors
///
/// A [`Rejection`] for the first check that fails.
pub fn verify_registration(
    rp: &RelyingParty,
    client_data_json: &[u8],
    attestation_object: &[u8],
) -> Result<NewCredential, Rejection> {
    check_client_data(client_data_json, "webauthn.create", rp)?;

    let object: Value =
        ciborium::de::from_reader(attestation_object).map_err(|_| Rejection::Malformed)?;
    let auth_data = map_entry(&object, &Value::Text("authData".to_owned()))
        .and_then(Value::as_bytes)
        .ok_or(Rejection::Malformed)?;

    let data = parse_authenticator_data(auth_data)?;
    check_authenticator_data(&data, rp)?;
    if data.flags & FLAG_ATTESTED == 0 {
        return Err(Rejection::Malformed);
    }

    // Attested credential data: a 16-byte model identifier, the id's length,
    // the id, then the COSE key. Extensions may follow the key, so its end is
    // found by reading it.
    let (_aaguid, rest) = data
        .rest
        .split_first_chunk::<16>()
        .ok_or(Rejection::Malformed)?;
    let (length, rest) = rest.split_first_chunk::<2>().ok_or(Rejection::Malformed)?;
    let length = usize::from(u16::from_be_bytes(*length));
    if length == 0 || length > MAX_CREDENTIAL_ID {
        return Err(Rejection::Malformed);
    }
    let (id, rest) = rest.split_at_checked(length).ok_or(Rejection::Malformed)?;

    let mut remaining = rest;
    let key: Value = ciborium::de::from_reader(&mut remaining).map_err(|_| Rejection::Malformed)?;
    let consumed = rest.len() - remaining.len();
    let public_key = rest.get(..consumed).ok_or(Rejection::Malformed)?;
    // Parsed now so a key that could never verify is refused at registration,
    // not at every sign-in after.
    PublicKey::from_cose(&key)?;

    Ok(NewCredential {
        id: id.to_vec(),
        public_key: public_key.to_vec(),
        sign_count: data.sign_count,
        backup_eligible: data.flags & FLAG_BACKUP_ELIGIBLE != 0,
        backed_up: data.flags & FLAG_BACKED_UP != 0,
    })
}

/// Checks a sign-in, §7.2, against a stored key, for a challenge the caller
/// has already matched.
///
/// The counter is not judged here: whether it went backwards is a comparison
/// with the stored one, which the caller holds.
///
/// # Errors
///
/// A [`Rejection`] for the first check that fails.
pub fn verify_assertion(
    rp: &RelyingParty,
    public_key: &[u8],
    client_data_json: &[u8],
    authenticator_data: &[u8],
    signature: &[u8],
) -> Result<Assertion, Rejection> {
    check_client_data(client_data_json, "webauthn.get", rp)?;
    let data = parse_authenticator_data(authenticator_data)?;
    check_authenticator_data(&data, rp)?;

    let key: Value = ciborium::de::from_reader(public_key).map_err(|_| Rejection::Malformed)?;
    let key = PublicKey::from_cose(&key)?;

    // What was signed: the authenticator data, then the hash of the client
    // data. The hash, not the JSON, binds the challenge and the origin.
    let mut signed = Vec::with_capacity(authenticator_data.len() + 32);
    signed.extend_from_slice(authenticator_data);
    signed.extend_from_slice(&Sha256::digest(client_data_json));
    key.verify(&signed, signature)?;

    Ok(Assertion {
        sign_count: data.sign_count,
        backed_up: data.flags & FLAG_BACKED_UP != 0,
    })
}

/// A public key this can verify with, read from COSE.
enum PublicKey {
    /// P-256, as the uncompressed point `04 ‖ x ‖ y`.
    Es256(Vec<u8>),
    /// Ed25519, the 32-byte key.
    Ed25519(Vec<u8>),
    /// RSA, modulus and exponent.
    Rs256 { n: Vec<u8>, e: Vec<u8> },
}

impl PublicKey {
    fn from_cose(key: &Value) -> Result<Self, Rejection> {
        let int = |label: i64| map_entry(key, &Value::Integer(label.into())).and_then(as_i64);
        let bytes = |label: i64| {
            map_entry(key, &Value::Integer(label.into()))
                .and_then(Value::as_bytes)
                .ok_or(Rejection::Malformed)
        };
        // Labels from RFC 9053: 1 is the key type and 3 the algorithm; the
        // negative ones mean different things for each key type.
        match (int(1), int(3)) {
            (Some(2), Some(ES256)) => {
                if int(-1) != Some(1) {
                    return Err(Rejection::UnsupportedKey);
                }
                let (x, y) = (bytes(-2)?, bytes(-3)?);
                if x.len() != 32 || y.len() != 32 {
                    return Err(Rejection::Malformed);
                }
                let mut point = Vec::with_capacity(65);
                point.push(0x04);
                point.extend_from_slice(x);
                point.extend_from_slice(y);
                Ok(Self::Es256(point))
            }
            (Some(1), Some(EDDSA)) => {
                if int(-1) != Some(6) {
                    return Err(Rejection::UnsupportedKey);
                }
                let x = bytes(-2)?;
                if x.len() != 32 {
                    return Err(Rejection::Malformed);
                }
                Ok(Self::Ed25519(x.clone()))
            }
            (Some(3), Some(RS256)) => {
                let (n, e) = (bytes(-1)?, bytes(-2)?);
                // RSA_PKCS1_2048_8192 refuses anything shorter when verifying;
                // refusing it here says so at registration.
                if n.len() < 256 {
                    return Err(Rejection::UnsupportedKey);
                }
                Ok(Self::Rs256 {
                    n: n.clone(),
                    e: e.clone(),
                })
            }
            _ => Err(Rejection::UnsupportedKey),
        }
    }

    fn verify(&self, message: &[u8], signature: &[u8]) -> Result<(), Rejection> {
        let outcome = match self {
            Self::Es256(point) => {
                UnparsedPublicKey::new(&ECDSA_P256_SHA256_ASN1, point).verify(message, signature)
            }
            Self::Ed25519(key) => UnparsedPublicKey::new(&ED25519, key).verify(message, signature),
            Self::Rs256 { n, e } => RsaPublicKeyComponents { n, e }.verify(
                &RSA_PKCS1_2048_8192_SHA256,
                message,
                signature,
            ),
        };
        outcome.map_err(|_| Rejection::BadSignature)
    }
}

/// The value under `label` in a CBOR map.
fn map_entry<'a>(map: &'a Value, label: &Value) -> Option<&'a Value> {
    map.as_map()?
        .iter()
        .find(|(key, _)| key == label)
        .map(|(_, value)| value)
}

fn as_i64(value: &Value) -> Option<i64> {
    value
        .as_integer()
        .and_then(|integer| i64::try_from(integer).ok())
}

#[cfg(test)]
mod tests {
    use aws_lc_rs::{
        rand::SystemRandom,
        signature::{
            ECDSA_P256_SHA256_ASN1_SIGNING, EcdsaKeyPair, Ed25519KeyPair, KeyPair as _,
            RSA_PKCS1_SHA256, RsaKeyPair,
        },
    };

    use super::*;

    fn rp() -> RelyingParty {
        RelyingParty::from_origin("https://files.example").expect("an origin")
    }

    fn cbor(value: &Value) -> Vec<u8> {
        let mut out = Vec::new();
        ciborium::ser::into_writer(value, &mut out).expect("serialisable");
        out
    }

    fn int(n: i64) -> Value {
        Value::Integer(n.into())
    }

    fn client_data(kind: &str, origin: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "type": kind,
            "challenge": "c2FtcGxl",
            "origin": origin,
            "crossOrigin": false,
        }))
        .expect("json")
    }

    fn authenticator_data(
        rp_id: &str,
        flags: u8,
        count: u32,
        attested: Option<(&[u8], &[u8])>,
    ) -> Vec<u8> {
        let mut out = Sha256::digest(rp_id.as_bytes()).to_vec();
        out.push(flags);
        out.extend_from_slice(&count.to_be_bytes());
        if let Some((id, key)) = attested {
            out.extend_from_slice(&[0; 16]);
            out.extend_from_slice(&u16::try_from(id.len()).expect("short").to_be_bytes());
            out.extend_from_slice(id);
            out.extend_from_slice(key);
        }
        out
    }

    fn attestation_object(auth_data: Vec<u8>) -> Vec<u8> {
        cbor(&Value::Map(vec![
            (Value::Text("fmt".into()), Value::Text("none".into())),
            (Value::Text("attStmt".into()), Value::Map(vec![])),
            (Value::Text("authData".into()), Value::Bytes(auth_data)),
        ]))
    }

    /// A software authenticator with one key, of each type this accepts.
    enum Key {
        Es256(EcdsaKeyPair),
        Ed25519(Ed25519KeyPair),
        Rs256(RsaKeyPair),
    }

    impl Key {
        fn es256() -> Self {
            Self::Es256(EcdsaKeyPair::generate(&ECDSA_P256_SHA256_ASN1_SIGNING).expect("a key"))
        }

        fn ed25519() -> Self {
            Self::Ed25519(Ed25519KeyPair::generate().expect("a key"))
        }

        fn rs256() -> Self {
            Self::Rs256(RsaKeyPair::generate(aws_lc_rs::rsa::KeySize::Rsa2048).expect("a key"))
        }

        fn cose(&self) -> Vec<u8> {
            let map = match self {
                Self::Es256(pair) => {
                    let point = pair.public_key().as_ref();
                    vec![
                        (int(1), int(2)),
                        (int(3), int(ES256)),
                        (int(-1), int(1)),
                        (int(-2), Value::Bytes(point[1..33].to_vec())),
                        (int(-3), Value::Bytes(point[33..65].to_vec())),
                    ]
                }
                Self::Ed25519(pair) => vec![
                    (int(1), int(1)),
                    (int(3), int(EDDSA)),
                    (int(-1), int(6)),
                    (int(-2), Value::Bytes(pair.public_key().as_ref().to_vec())),
                ],
                Self::Rs256(pair) => {
                    let components =
                        aws_lc_rs::rsa::PublicKeyComponents::<Vec<u8>>::from(pair.public_key());
                    vec![
                        (int(1), int(3)),
                        (int(3), int(RS256)),
                        (int(-1), Value::Bytes(components.n)),
                        (int(-2), Value::Bytes(components.e)),
                    ]
                }
            };
            cbor(&Value::Map(map))
        }

        fn sign(&self, message: &[u8]) -> Vec<u8> {
            let rng = SystemRandom::new();
            match self {
                Self::Es256(pair) => pair.sign(&rng, message).expect("signed").as_ref().to_vec(),
                Self::Ed25519(pair) => pair.sign(message).as_ref().to_vec(),
                Self::Rs256(pair) => {
                    let mut signature = vec![0; pair.public_modulus_len()];
                    pair.sign(&RSA_PKCS1_SHA256, &rng, message, &mut signature)
                        .expect("signed");
                    signature
                }
            }
        }
    }

    fn register(key: &Key, flags: u8) -> Result<NewCredential, Rejection> {
        let data = authenticator_data(
            "files.example",
            flags,
            0,
            Some((b"credential", &key.cose())),
        );
        verify_registration(
            &rp(),
            &client_data("webauthn.create", "https://files.example"),
            &attestation_object(data),
        )
    }

    fn sign_in(
        key: &Key,
        stored: &[u8],
        origin: &str,
        rp_id: &str,
        flags: u8,
    ) -> Result<Assertion, Rejection> {
        let client = client_data("webauthn.get", origin);
        let data = authenticator_data(rp_id, flags, 7, None);
        let mut signed = data.clone();
        signed.extend_from_slice(&Sha256::digest(&client));
        let signature = key.sign(&signed);
        verify_assertion(&rp(), stored, &client, &data, &signature)
    }

    const VERIFIED: u8 = FLAG_USER_PRESENT | FLAG_USER_VERIFIED;

    #[test]
    fn every_accepted_key_type_registers_and_signs_in() {
        for key in [Key::es256(), Key::ed25519(), Key::rs256()] {
            let credential = register(&key, VERIFIED | FLAG_ATTESTED | FLAG_BACKUP_ELIGIBLE)
                .expect("registered");
            assert_eq!(credential.id, b"credential");
            assert!(credential.backup_eligible);
            let assertion = sign_in(
                &key,
                &credential.public_key,
                "https://files.example",
                "files.example",
                VERIFIED,
            )
            .expect("signed in");
            assert_eq!(assertion.sign_count, 7);
        }
    }

    #[test]
    fn extensions_after_the_key_are_not_mistaken_for_it() {
        let key = Key::es256();
        let mut tail = key.cose();
        let stored = tail.clone();
        tail.extend_from_slice(&cbor(&Value::Map(vec![(
            Value::Text("credProtect".into()),
            int(2),
        )])));
        let data = authenticator_data(
            "files.example",
            VERIFIED | FLAG_ATTESTED | 0x80,
            0,
            Some((b"credential", &tail)),
        );
        let credential = verify_registration(
            &rp(),
            &client_data("webauthn.create", "https://files.example"),
            &attestation_object(data),
        )
        .expect("registered");
        assert_eq!(credential.public_key, stored);
    }

    #[test]
    fn a_person_who_was_not_verified_is_refused() {
        let key = Key::es256();
        assert_eq!(
            register(&key, FLAG_USER_PRESENT | FLAG_ATTESTED).err(),
            Some(Rejection::NotVerified)
        );
        let credential = register(&key, VERIFIED | FLAG_ATTESTED).expect("registered");
        assert_eq!(
            sign_in(
                &key,
                &credential.public_key,
                "https://files.example",
                "files.example",
                FLAG_USER_PRESENT
            )
            .err(),
            Some(Rejection::NotVerified)
        );
    }

    #[test]
    fn another_site_cannot_use_the_answer() {
        let key = Key::es256();
        let credential = register(&key, VERIFIED | FLAG_ATTESTED).expect("registered");
        let stored = &credential.public_key;
        assert_eq!(
            sign_in(
                &key,
                stored,
                "https://files.example.evil",
                "files.example",
                VERIFIED
            )
            .err(),
            Some(Rejection::WrongOrigin)
        );
        assert_eq!(
            sign_in(
                &key,
                stored,
                "https://files.example",
                "evil.example",
                VERIFIED
            )
            .err(),
            Some(Rejection::WrongRelyingParty)
        );
    }

    #[test]
    fn another_keys_signature_does_not_verify() {
        let key = Key::es256();
        let credential = register(&key, VERIFIED | FLAG_ATTESTED).expect("registered");
        assert_eq!(
            sign_in(
                &Key::es256(),
                &credential.public_key,
                "https://files.example",
                "files.example",
                VERIFIED
            )
            .err(),
            Some(Rejection::BadSignature)
        );
    }

    #[test]
    fn a_registration_answer_is_not_a_sign_in() {
        let key = Key::es256();
        let credential = register(&key, VERIFIED | FLAG_ATTESTED).expect("registered");
        let client = client_data("webauthn.create", "https://files.example");
        let data = authenticator_data("files.example", VERIFIED, 1, None);
        let mut signed = data.clone();
        signed.extend_from_slice(&Sha256::digest(&client));
        assert_eq!(
            verify_assertion(
                &rp(),
                &credential.public_key,
                &client,
                &data,
                &key.sign(&signed)
            )
            .err(),
            Some(Rejection::WrongType)
        );
    }

    #[test]
    fn the_origin_is_compared_as_the_browser_writes_it() {
        let rp = RelyingParty::from_origin("https://Files.Example:443/").expect("an origin");
        assert_eq!(rp.id, "files.example");
        assert_eq!(rp.origin, "https://files.example");
        let local = RelyingParty::from_origin("http://localhost:8080").expect("an origin");
        assert_eq!(local.origin, "http://localhost:8080");
        assert!(RelyingParty::from_origin("not a url").is_none());
    }

    #[test]
    fn truncated_data_is_malformed_not_a_panic() {
        assert_eq!(
            verify_assertion(
                &rp(),
                &[],
                &client_data("webauthn.get", "https://files.example"),
                &[0; 10],
                &[]
            )
            .err(),
            Some(Rejection::Malformed)
        );
        let data = authenticator_data("files.example", VERIFIED | FLAG_ATTESTED, 0, None);
        assert_eq!(
            verify_registration(
                &rp(),
                &client_data("webauthn.create", "https://files.example"),
                &attestation_object(data),
            )
            .err(),
            Some(Rejection::Malformed)
        );
    }
}
