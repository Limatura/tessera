//! A software passkey, for the tests that need a browser's half of WebAuthn.
//!
//! It answers the server's options the way a platform authenticator does: an
//! ES256 key, the user present and verified, and the JSON that
//! `PublicKeyCredential.toJSON()` writes. Built from the specification, not from
//! the server's code, so a misreading on one side does not cancel out on the
//! other.

use aws_lc_rs::{
    rand::SystemRandom,
    signature::{ECDSA_P256_SHA256_ASN1_SIGNING, EcdsaKeyPair, KeyPair as _},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use ciborium::Value;
use sha2::{Digest as _, Sha256};

/// User present, user verified.
const VERIFIED: u8 = 0x01 | 0x04;
/// Attested credential data follows.
const ATTESTED: u8 = 0x40;

/// One passkey, held in memory.
pub(crate) struct Authenticator {
    key: EcdsaKeyPair,
    /// The id it registered under.
    pub(crate) credential_id: Vec<u8>,
    /// The account handle the server gave it.
    user_handle: Vec<u8>,
    /// Its signature counter, or `None` for one that keeps none, as synced
    /// passkeys do.
    counter: Option<u32>,
    /// The page it believes it is on.
    origin: String,
}

fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

fn text(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| panic!("`{key}` in {value}"))
        .to_owned()
}

fn cbor(value: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    ciborium::ser::into_writer(value, &mut out).expect("serialisable");
    out
}

impl Authenticator {
    /// A new key, for a page at `origin`.
    pub(crate) fn new(origin: &str) -> Self {
        let mut credential_id = vec![0_u8; 16];
        aws_lc_rs::rand::fill(&mut credential_id).expect("random bytes");
        Self {
            key: EcdsaKeyPair::generate(&ECDSA_P256_SHA256_ASN1_SIGNING).expect("a key"),
            credential_id,
            user_handle: Vec::new(),
            counter: None,
            origin: origin.to_owned(),
        }
    }

    /// The same key, counting signatures from `start`, as a hardware key does.
    pub(crate) const fn with_counter(mut self, start: u32) -> Self {
        self.counter = Some(start);
        self
    }

    fn client_data(&self, kind: &str, challenge: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "type": kind,
            "challenge": challenge,
            "origin": self.origin,
            "crossOrigin": false,
        }))
        .expect("json")
    }

    fn authenticator_data(&self, rp_id: &str, flags: u8, attested: bool) -> Vec<u8> {
        let mut out = Sha256::digest(rp_id.as_bytes()).to_vec();
        out.push(flags | if attested { ATTESTED } else { 0 });
        out.extend_from_slice(&self.counter.unwrap_or(0).to_be_bytes());
        if attested {
            let point = self.key.public_key().as_ref();
            let (x, y) = point
                .get(1..33)
                .zip(point.get(33..65))
                .expect("an uncompressed P-256 point");
            let int = |n: i64| Value::Integer(n.into());
            let key = cbor(&Value::Map(vec![
                (int(1), int(2)),
                (int(3), int(-7)),
                (int(-1), int(1)),
                (int(-2), Value::Bytes(x.to_vec())),
                (int(-3), Value::Bytes(y.to_vec())),
            ]));
            out.extend_from_slice(&[0; 16]);
            out.extend_from_slice(
                &u16::try_from(self.credential_id.len())
                    .expect("short")
                    .to_be_bytes(),
            );
            out.extend_from_slice(&self.credential_id);
            out.extend_from_slice(&key);
        }
        out
    }

    /// Answers creation options, as `navigator.credentials.create` would.
    pub(crate) fn register(&mut self, options: &serde_json::Value) -> serde_json::Value {
        let rp_id = text(options.get("rp").expect("rp"), "id");
        let handle = text(options.get("user").expect("user"), "id");
        self.user_handle = URL_SAFE_NO_PAD.decode(handle).expect("a base64url handle");
        let client = self.client_data("webauthn.create", &text(options, "challenge"));
        let data = self.authenticator_data(&rp_id, VERIFIED, true);
        let attestation = cbor(&Value::Map(vec![
            (Value::Text("fmt".into()), Value::Text("none".into())),
            (Value::Text("attStmt".into()), Value::Map(vec![])),
            (Value::Text("authData".into()), Value::Bytes(data)),
        ]));
        serde_json::json!({
            "id": b64(&self.credential_id),
            "rawId": b64(&self.credential_id),
            "type": "public-key",
            "authenticatorAttachment": "platform",
            "clientExtensionResults": {},
            "response": {
                "clientDataJSON": b64(&client),
                "attestationObject": b64(&attestation),
                "transports": ["internal"],
            },
        })
    }

    /// Answers request options, as `navigator.credentials.get` would.
    pub(crate) fn sign_in(&mut self, options: &serde_json::Value) -> serde_json::Value {
        self.sign_in_with_flags(options, VERIFIED)
    }

    /// The same, with whatever flags the test wants the authenticator to claim.
    pub(crate) fn sign_in_with_flags(
        &mut self,
        options: &serde_json::Value,
        flags: u8,
    ) -> serde_json::Value {
        if let Some(counter) = self.counter.as_mut() {
            *counter += 1;
        }
        let client = self.client_data("webauthn.get", &text(options, "challenge"));
        let data = self.authenticator_data(&text(options, "rpId"), flags, false);
        let mut signed = data.clone();
        signed.extend_from_slice(&Sha256::digest(&client));
        let signature = self
            .key
            .sign(&SystemRandom::new(), &signed)
            .expect("signed");
        serde_json::json!({
            "id": b64(&self.credential_id),
            "rawId": b64(&self.credential_id),
            "type": "public-key",
            "clientExtensionResults": {},
            "response": {
                "clientDataJSON": b64(&client),
                "authenticatorData": b64(&data),
                "signature": b64(signature.as_ref()),
                "userHandle": b64(&self.user_handle),
            },
        })
    }

    /// Moves the key to another page, as a phishing site would want it used.
    pub(crate) fn visit(&mut self, origin: &str) {
        origin.clone_into(&mut self.origin);
    }

    /// Winds the counter back, as a copy of the key made earlier would be.
    pub(crate) const fn rewind(&mut self, by: u32) {
        if let Some(counter) = self.counter.as_mut() {
            *counter -= by;
        }
    }
}
