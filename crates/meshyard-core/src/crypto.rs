use crate::{domain::*, wire::v1::*};
use ed25519_dalek::Signer as _;

pub const GENESIS_DOMAIN: &[u8] = b"meshyard-genesis-v1";
pub const EVENT_DOMAIN: &[u8] = b"meshyard-ev-v1";
pub const WORKSPACE_DOMAIN: &[u8] = b"meshyard-ws-v1";
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CryptoError {
    #[error("malformed protocol encoding")]
    Encoding,
    #[error("invalid protocol field")]
    InvalidField,
    #[error("invalid signature")]
    Signature,
    #[error("identity does not match owner")]
    OwnerMismatch,
    #[error("unsupported protocol version")]
    Version,
    #[error("invalid frame header")]
    Frame,
}
fn encoding(error: postcard::Error) -> CryptoError {
    if error == postcard::Error::DeserializeBadUtf8 {
        CryptoError::InvalidField
    } else {
        CryptoError::Encoding
    }
}
pub trait Signer: Send + Sync {
    fn device_id(&self) -> DeviceId;
    fn sign(&self, message: &[u8]) -> Result<[u8; 64], CryptoError>;
}
pub trait Verifier {
    fn verify(
        &self,
        author: DeviceId,
        message: &[u8],
        signature: &[u8; 64],
    ) -> Result<(), CryptoError>;
}
pub struct Ed25519Verifier;
impl Verifier for Ed25519Verifier {
    fn verify(
        &self,
        author: DeviceId,
        message: &[u8],
        signature: &[u8; 64],
    ) -> Result<(), CryptoError> {
        let key = ed25519_dalek::VerifyingKey::from_bytes(&author.0)
            .map_err(|_| CryptoError::Signature)?;
        key.verify_strict(message, &ed25519_dalek::Signature::from_bytes(signature))
            .map_err(|_| CryptoError::Signature)
    }
}
/// No Serialize/TS implementation; dalek zeroizes the key on drop.
pub struct SecretKey(ed25519_dalek::SigningKey);
impl std::fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretKey([REDACTED])")
    }
}
impl SecretKey {
    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        Self(ed25519_dalek::SigningKey::from_bytes(bytes))
    }
    /// Only for an explicitly selected OS/file key store; never pass to SQL or IPC.
    pub fn with_secret_bytes<T>(&self, use_bytes: impl FnOnce(&[u8; 32]) -> T) -> T {
        use_bytes(self.0.as_bytes())
    }
}
impl Signer for SecretKey {
    fn device_id(&self) -> DeviceId {
        DeviceId(self.0.verifying_key().to_bytes())
    }
    fn sign(&self, message: &[u8]) -> Result<[u8; 64], CryptoError> {
        Ok(self.0.sign(message).to_bytes())
    }
}
pub fn domain_message(domain: &[u8], bytes: &[u8]) -> Vec<u8> {
    [domain, bytes].concat()
}
pub fn workspace_id(genesis_bytes: &[u8]) -> WorkspaceId {
    let mut hash = blake3::Hasher::new();
    hash.update(WORKSPACE_DOMAIN);
    hash.update(genesis_bytes);
    let mut id = [0; 16];
    id.copy_from_slice(&hash.finalize().as_bytes()[..16]);
    WorkspaceId(id)
}
pub fn sign_genesis(
    mut genesis: Genesis,
    signer: &dyn Signer,
) -> Result<SignedGenesisV1, CryptoError> {
    if genesis.owner != signer.device_id() {
        return Err(CryptoError::OwnerMismatch);
    }
    genesis.name = sanitize_for_signing(&genesis.name, 64);
    let genesis_bytes = postcard::to_stdvec(&GenesisV1::from(genesis)).map_err(encoding)?;
    let sig = signer.sign(&domain_message(GENESIS_DOMAIN, &genesis_bytes))?;
    Ok(SignedGenesisV1 { genesis_bytes, sig })
}
pub fn verify_genesis(
    raw: &SignedGenesisV1,
    verifier: &dyn Verifier,
) -> Result<(WorkspaceId, Genesis), CryptoError> {
    if raw.genesis_bytes.len() > MAX_FRAME {
        return Err(CryptoError::Encoding);
    }
    let (genesis, rest) =
        postcard::take_from_bytes::<GenesisV1>(&raw.genesis_bytes).map_err(encoding)?;
    if !rest.is_empty() {
        return Err(CryptoError::Encoding);
    }
    verifier.verify(
        DeviceId(genesis.owner),
        &domain_message(GENESIS_DOMAIN, &raw.genesis_bytes),
        &raw.sig,
    )?;
    if genesis.v != 1 {
        return Err(CryptoError::Version);
    }
    validate_received(&genesis.name, 64).map_err(|_| CryptoError::InvalidField)?;
    Ok((workspace_id(&raw.genesis_bytes), genesis.into()))
}
/// Policy and sanitation are mandatory on the sender path. Remote verification never calls this.
pub fn sign_event(
    mut event: Event,
    signer: &dyn Signer,
    policy: &crate::SharingPolicy,
) -> Result<SignedEventV1, CryptoError> {
    if event.author != signer.device_id() {
        return Err(CryptoError::Signature);
    }
    event.body = policy.filter(event.body);
    event.body.sanitize_for_signing();
    event
        .validate_received()
        .map_err(|_| CryptoError::InvalidField)?;
    let header = EventHeaderV1 {
        v: 1,
        workspace: event.workspace.0,
        author: event.author.0,
        seq: event.seq,
        ts_ms: event.ts_ms,
        membership_epoch: event.membership_epoch,
    };
    let mut signed = postcard::to_stdvec(&header).map_err(encoding)?;
    signed.extend(postcard::to_stdvec(&EventBodyV1::from(event.body)).map_err(encoding)?);
    let sig = signer.sign(&domain_message(EVENT_DOMAIN, &signed))?;
    Ok(SignedEventV1 { signed, sig })
}
/// Authenticated/decoded fields, deliberately not yet field-validated. The store checks
/// duplicates/equivocation before calling `validate_received` and projecting `into_event`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedEvent {
    event: Event,
    version: u8,
}
impl std::ops::Deref for VerifiedEvent {
    type Target = Event;
    fn deref(&self) -> &Event {
        &self.event
    }
}
impl VerifiedEvent {
    pub fn validate_received(&self) -> Result<(), CryptoError> {
        if self.version != 1 {
            return Err(CryptoError::Version);
        }
        self.event
            .validate_received()
            .map_err(|_| CryptoError::InvalidField)
    }
    pub fn into_event(self) -> Event {
        self.event
    }
}
/// Header → raw signature → body consuming all bytes. Field validation MUST follow store dedupe.
pub fn verify_and_decode(
    raw: &SignedEventV1,
    verifier: &dyn Verifier,
) -> Result<VerifiedEvent, CryptoError> {
    if raw.signed.len() > MAX_FRAME {
        return Err(CryptoError::Encoding);
    }
    let (h, rest) = postcard::take_from_bytes::<EventHeaderV1>(&raw.signed).map_err(encoding)?;
    verifier.verify(
        DeviceId(h.author),
        &domain_message(EVENT_DOMAIN, &raw.signed),
        &raw.sig,
    )?;
    let (body, rest) = postcard::take_from_bytes::<EventBodyV1>(rest).map_err(encoding)?;
    if !rest.is_empty() {
        return Err(CryptoError::Encoding);
    }
    Ok(VerifiedEvent {
        version: h.v,
        event: Event {
            workspace: WorkspaceId(h.workspace),
            author: DeviceId(h.author),
            seq: h.seq,
            ts_ms: h.ts_ms,
            membership_epoch: h.membership_epoch,
            body: body.into(),
        },
    })
}
/// Decode outer frame strictly without accepting trailing bytes.
pub fn decode_signed_event(bytes: &[u8]) -> Result<SignedEventV1, CryptoError> {
    if bytes.len() > MAX_FRAME {
        return Err(CryptoError::Encoding);
    }
    let (value, rest) = postcard::take_from_bytes(bytes).map_err(encoding)?;
    if !rest.is_empty() {
        return Err(CryptoError::Encoding);
    }
    Ok(value)
}
