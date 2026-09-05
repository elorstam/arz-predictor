//! Production licensing client, local Ed25519 verification, and offline policy.
//!
//! The production binary can only verify tokens. Signing imports and helpers are
//! compiled exclusively for tests; the private production key remains in Supabase.

use base64::{
    engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD},
    Engine as _,
};
use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
#[cfg(test)]
use ed25519_dalek::{Signer, SigningKey};
use rand_core::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, time::Duration as StdDuration};

pub const TOKEN_VERSION: u32 = 1;
pub const DEFAULT_SUPABASE_URL: &str = "https://dlxixkqlamuqsfgowqtc.supabase.co";
pub const SUPABASE_URL: &str = match option_env!("SUPABASE_URL") {
    Some(value) => value,
    None => DEFAULT_SUPABASE_URL,
};
pub const SUPABASE_PUBLISHABLE_KEY: &str = match option_env!("SUPABASE_PUBLISHABLE_KEY") {
    Some(value) => value,
    None => "",
};
pub const PUBLIC_VERIFYING_KEY_B64: &str = match option_env!("ARZ_LICENSE_VERIFYING_KEY_B64") {
    Some(value) => value,
    None => "",
};

const ED25519_SPKI_PREFIX: [u8; 12] = [
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum LicensePlan {
    MONTHLY,
    YEARLY,
    LIFETIME,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum LicenseStatus {
    UNUSED,
    ACTIVE,
    SUSPENDED,
    REVOKED,
    EXPIRED,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DesktopLicenseState {
    UNLICENSED,
    VERIFYING,
    ACTIVE,
    OFFLINE_GRACE,
    EXPIRED,
    SUSPENDED,
    REVOKED,
    DEVICE_MISMATCH,
    INVALID_TOKEN,
    CLOCK_ANOMALY,
    SERVER_UNAVAILABLE,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LicenseError {
    InvalidKey,
    InvalidToken,
    UnsupportedTokenVersion,
    DeviceMismatch,
    Expired,
    ClockAnomaly,
    Suspended,
    Revoked,
    DeviceAlreadyBound,
    LicenseNotFound,
    ServerUnavailable,
    Configuration,
    Io(String),
}

impl LicenseError {
    pub fn authoritative_state(&self) -> Option<DesktopLicenseState> {
        match self {
            Self::Suspended => Some(DesktopLicenseState::SUSPENDED),
            Self::Revoked => Some(DesktopLicenseState::REVOKED),
            Self::Expired => Some(DesktopLicenseState::EXPIRED),
            Self::DeviceMismatch => Some(DesktopLicenseState::DEVICE_MISMATCH),
            Self::InvalidToken | Self::UnsupportedTokenVersion | Self::LicenseNotFound => {
                Some(DesktopLicenseState::INVALID_TOKEN)
            }
            _ => None,
        }
    }
}

impl std::fmt::Display for LicenseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::InvalidKey => "Lisans anahtarı geçersiz.",
            Self::InvalidToken => "Aktivasyon tokenı geçersiz.",
            Self::UnsupportedTokenVersion => "Desteklenmeyen lisans tokenı sürümü.",
            Self::DeviceMismatch => "Lisans bu cihaza bağlı değil.",
            Self::Expired => "Lisansın süresi dolmuş.",
            Self::ClockAnomaly => {
                "Sistem saatinde şüpheli bir geri alma algılandı. Çevrimiçi doğrulama gerekli."
            }
            Self::Suspended => "Lisans askıya alınmış.",
            Self::Revoked => "Lisans iptal edilmiş.",
            Self::DeviceAlreadyBound => "Lisans başka bir cihaza bağlı.",
            Self::LicenseNotFound => "Lisans bulunamadı.",
            Self::ServerUnavailable => "Lisans sunucusuna ulaşılamıyor.",
            Self::Configuration => "Lisans servisi public yapılandırması eksik.",
            Self::Io(_) => "Yerel lisans kaydı okunamadı.",
        };
        f.write_str(message)
    }
}
impl std::error::Error for LicenseError {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LicenseClaims {
    pub license_id: String,
    pub plan: LicensePlan,
    pub status: LicenseStatus,
    pub device_fingerprint_hash: String,
    pub device_binding_version: i64,
    pub issued_at: DateTime<Utc>,
    pub license_expires_at: Option<DateTime<Utc>>,
    pub online_revalidate_after: DateTime<Utc>,
    pub offline_grace_until: DateTime<Utc>,
    pub token_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredActivation {
    pub token: String,
    #[serde(default)]
    pub activated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_verified_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_successful_server_time: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_accepted_wall_clock: Option<DateTime<Utc>>,
    #[serde(default)]
    pub authoritative_state: Option<DesktopLicenseState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerLicense {
    pub id: String,
    pub plan: LicensePlan,
    pub status: LicenseStatus,
    pub activated_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub device_bound: bool,
    pub device_binding_version: i64,
    pub last_verified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerLicenseResponse {
    pub ok: bool,
    pub license: ServerLicense,
    pub activation_token: String,
    pub server_time: DateTime<Utc>,
    pub online_revalidate_after: DateTime<Utc>,
    pub offline_grace_until: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct ServerErrorResponse {
    code: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LicenseSnapshot {
    pub state: DesktopLicenseState,
    pub claims: Option<LicenseClaims>,
    pub activated_at: Option<DateTime<Utc>>,
    pub last_verified_at: Option<DateTime<Utc>>,
    pub message: String,
}

impl LicenseSnapshot {
    pub fn unlicensed() -> Self {
        Self {
            state: DesktopLicenseState::UNLICENSED,
            claims: None,
            activated_at: None,
            last_verified_at: None,
            message: "Bir lisans anahtarıyla aktivasyon yapın.".into(),
        }
    }

    fn blocked(
        state: DesktopLicenseState,
        claims: Option<LicenseClaims>,
        stored: Option<&StoredActivation>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            state,
            claims,
            activated_at: stored.and_then(|value| value.activated_at),
            last_verified_at: stored.and_then(|value| value.last_verified_at),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LicenseConfig {
    pub clock_tolerance_seconds: i64,
}
impl Default for LicenseConfig {
    fn default() -> Self {
        Self {
            clock_tolerance_seconds: 300,
        }
    }
}

pub trait LicenseProvider {
    fn activate(
        &self,
        normalized_key: &str,
        device_hash: &str,
    ) -> Result<ServerLicenseResponse, LicenseError>;
    fn verify(
        &self,
        activation_token: &str,
        device_hash: &str,
    ) -> Result<ServerLicenseResponse, LicenseError>;
}

#[derive(Debug, Clone)]
pub struct SupabaseLicenseProvider {
    base_url: String,
    publishable_key: String,
    app_version: String,
}

impl SupabaseLicenseProvider {
    pub fn production() -> Self {
        Self {
            base_url: SUPABASE_URL.trim_end_matches('/').to_owned(),
            publishable_key: SUPABASE_PUBLISHABLE_KEY.to_owned(),
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
        }
    }

    fn post<T: Serialize>(
        &self,
        function: &str,
        body: &T,
    ) -> Result<ServerLicenseResponse, LicenseError> {
        if self.base_url.is_empty() || self.publishable_key.is_empty() {
            return Err(LicenseError::Configuration);
        }
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(StdDuration::from_secs(5))
            .timeout(StdDuration::from_secs(12))
            .build()
            .map_err(|_| LicenseError::ServerUnavailable)?;
        let response = client
            .post(format!("{}/functions/v1/{function}", self.base_url))
            .header("apikey", &self.publishable_key)
            .bearer_auth(&self.publishable_key)
            .json(body)
            .send()
            .map_err(|_| LicenseError::ServerUnavailable)?;
        let status = response.status();
        if status.is_success() {
            return response
                .json::<ServerLicenseResponse>()
                .map_err(|_| LicenseError::InvalidToken);
        }
        let code = response
            .json::<ServerErrorResponse>()
            .map(|value| value.code)
            .unwrap_or_else(|_| "SERVER_ERROR".into());
        Err(error_from_server_code(&code, status.is_server_error()))
    }
}

#[derive(Serialize)]
struct ActivationRequest<'a> {
    license_key: &'a str,
    device_fingerprint_hash: &'a str,
    app_version: &'a str,
    token_version: u32,
}

#[derive(Serialize)]
struct VerifyRequest<'a> {
    activation_token: &'a str,
    device_fingerprint_hash: &'a str,
    app_version: &'a str,
}

impl LicenseProvider for SupabaseLicenseProvider {
    fn activate(
        &self,
        normalized_key: &str,
        device_hash: &str,
    ) -> Result<ServerLicenseResponse, LicenseError> {
        self.post(
            "license-activate",
            &ActivationRequest {
                license_key: normalized_key,
                device_fingerprint_hash: device_hash,
                app_version: &self.app_version,
                token_version: TOKEN_VERSION,
            },
        )
    }

    fn verify(
        &self,
        activation_token: &str,
        device_hash: &str,
    ) -> Result<ServerLicenseResponse, LicenseError> {
        self.post(
            "license-verify",
            &VerifyRequest {
                activation_token,
                device_fingerprint_hash: device_hash,
                app_version: &self.app_version,
            },
        )
    }
}

fn error_from_server_code(code: &str, server_error: bool) -> LicenseError {
    match code {
        "INVALID_LICENSE_KEY" => LicenseError::InvalidKey,
        "LICENSE_NOT_FOUND" => LicenseError::LicenseNotFound,
        "LICENSE_SUSPENDED" => LicenseError::Suspended,
        "LICENSE_REVOKED" => LicenseError::Revoked,
        "LICENSE_EXPIRED" => LicenseError::Expired,
        "DEVICE_ALREADY_BOUND" => LicenseError::DeviceAlreadyBound,
        "DEVICE_MISMATCH" => LicenseError::DeviceMismatch,
        "INVALID_TOKEN" => LicenseError::InvalidToken,
        "UNSUPPORTED_TOKEN_VERSION" => LicenseError::UnsupportedTokenVersion,
        "SERVER_ERROR" => LicenseError::ServerUnavailable,
        _ if server_error => LicenseError::ServerUnavailable,
        _ => LicenseError::InvalidToken,
    }
}

pub fn normalize_key(raw: &str) -> Result<String, LicenseError> {
    let compact: String = raw
        .trim()
        .chars()
        .filter(|character| !character.is_ascii_whitespace() && *character != '-')
        .map(|character| character.to_ascii_uppercase())
        .collect();
    if compact.len() != 20
        || !compact.starts_with("ARZP")
        || !compact
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        return Err(LicenseError::InvalidKey);
    }
    Ok(format!(
        "ARZP-{}-{}-{}-{}",
        &compact[4..8],
        &compact[8..12],
        &compact[12..16],
        &compact[16..20]
    ))
}

pub fn hash_license_key(normalized: &str) -> String {
    format!("{:x}", Sha256::digest(normalized.as_bytes()))
}

pub fn device_fingerprint_hash(app_data_root: &Path) -> Result<String, LicenseError> {
    let path = app_data_root.join("installation.id");
    let installation = match fs::read_to_string(&path) {
        Ok(value) if !value.trim().is_empty() => value,
        _ => {
            let mut bytes = [0_u8; 32];
            let mut rng = rand_core::OsRng;
            rng.try_fill_bytes(&mut bytes)
                .map_err(|error| LicenseError::Io(error.to_string()))?;
            let value = URL_SAFE_NO_PAD.encode(bytes);
            fs::create_dir_all(app_data_root)
                .map_err(|error| LicenseError::Io(error.to_string()))?;
            fs::write(&path, &value).map_err(|error| LicenseError::Io(error.to_string()))?;
            value
        }
    };
    let mut hasher = Sha256::new();
    hasher.update("ARZ-PREDICTOR-DEVICE-V1");
    hasher.update(std::env::consts::OS);
    hasher.update(installation.trim().as_bytes());
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn verifying_key_from_spki_b64(value: &str) -> Result<VerifyingKey, LicenseError> {
    let decoded = URL_SAFE_NO_PAD
        .decode(value)
        .or_else(|_| URL_SAFE.decode(value))
        .map_err(|_| LicenseError::InvalidToken)?;
    if decoded.len() != ED25519_SPKI_PREFIX.len() + 32
        || decoded[..ED25519_SPKI_PREFIX.len()] != ED25519_SPKI_PREFIX
    {
        return Err(LicenseError::InvalidToken);
    }
    let raw: [u8; 32] = decoded[ED25519_SPKI_PREFIX.len()..]
        .try_into()
        .map_err(|_| LicenseError::InvalidToken)?;
    VerifyingKey::from_bytes(&raw).map_err(|_| LicenseError::InvalidToken)
}

#[derive(Deserialize)]
struct TokenHeader {
    alg: String,
    typ: String,
    v: u32,
}

pub fn verify_token_signature(
    token: &str,
    verifying_key: &VerifyingKey,
) -> Result<LicenseClaims, LicenseError> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 || parts.iter().any(|part| part.is_empty()) {
        return Err(LicenseError::InvalidToken);
    }
    let header: TokenHeader = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(parts[0])
            .map_err(|_| LicenseError::InvalidToken)?,
    )
    .map_err(|_| LicenseError::InvalidToken)?;
    if header.alg != "Ed25519" || header.typ != "ARZ-LICENSE" || header.v != 1 {
        return Err(LicenseError::InvalidToken);
    }
    let signature_bytes = URL_SAFE_NO_PAD
        .decode(parts[2])
        .map_err(|_| LicenseError::InvalidToken)?;
    let signature =
        Signature::from_slice(&signature_bytes).map_err(|_| LicenseError::InvalidToken)?;
    verifying_key
        .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature)
        .map_err(|_| LicenseError::InvalidToken)?;
    let claims: LicenseClaims = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(parts[1])
            .map_err(|_| LicenseError::InvalidToken)?,
    )
    .map_err(|_| LicenseError::InvalidToken)?;
    if claims.token_version != TOKEN_VERSION {
        return Err(LicenseError::UnsupportedTokenVersion);
    }
    Ok(claims)
}

pub fn validate_claims(
    claims: &LicenseClaims,
    device_hash: &str,
    now: DateTime<Utc>,
    config: LicenseConfig,
) -> Result<(), LicenseError> {
    if claims.license_id.trim().is_empty()
        || claims.device_binding_version < 0
        || claims.online_revalidate_after < claims.issued_at
        || claims.offline_grace_until < claims.online_revalidate_after
        || claims.issued_at > now + Duration::seconds(config.clock_tolerance_seconds)
        || (claims.plan == LicensePlan::LIFETIME && claims.license_expires_at.is_some())
        || (claims.plan != LicensePlan::LIFETIME && claims.license_expires_at.is_none())
    {
        return Err(LicenseError::InvalidToken);
    }
    if claims.device_fingerprint_hash != device_hash {
        return Err(LicenseError::DeviceMismatch);
    }
    match claims.status {
        LicenseStatus::ACTIVE => {}
        LicenseStatus::SUSPENDED => return Err(LicenseError::Suspended),
        LicenseStatus::REVOKED => return Err(LicenseError::Revoked),
        LicenseStatus::EXPIRED => return Err(LicenseError::Expired),
        LicenseStatus::UNUSED => return Err(LicenseError::InvalidToken),
    }
    if claims
        .license_expires_at
        .is_some_and(|expires_at| now >= expires_at)
    {
        return Err(LicenseError::Expired);
    }
    Ok(())
}

fn trusted_clock(stored: &StoredActivation) -> Option<DateTime<Utc>> {
    [
        stored.last_successful_server_time,
        stored.last_accepted_wall_clock,
    ]
    .into_iter()
    .flatten()
    .max()
}

pub fn clock_rollback_detected(
    stored: &StoredActivation,
    now: DateTime<Utc>,
    config: LicenseConfig,
) -> bool {
    trusted_clock(stored)
        .is_some_and(|trusted| now < trusted - Duration::seconds(config.clock_tolerance_seconds))
}

pub fn online_revalidation_due(claims: &LicenseClaims, now: DateTime<Utc>) -> bool {
    now >= claims.online_revalidate_after
}

pub fn write_local_activation(
    path: &Path,
    activation: &StoredActivation,
) -> Result<(), LicenseError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| LicenseError::Io(error.to_string()))?;
    }
    let bytes = serde_json::to_vec_pretty(activation)
        .map_err(|error| LicenseError::Io(error.to_string()))?;
    fs::write(path, bytes).map_err(|error| LicenseError::Io(error.to_string()))
}

pub fn read_local_activation(path: &Path) -> Result<Option<StoredActivation>, LicenseError> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| LicenseError::InvalidToken),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(LicenseError::Io(error.to_string())),
    }
}

pub struct LicenseManager<P> {
    provider: P,
    verifying_key: VerifyingKey,
    config: LicenseConfig,
}

impl<P: LicenseProvider> LicenseManager<P> {
    pub fn new(provider: P, verifying_key: VerifyingKey, config: LicenseConfig) -> Self {
        Self {
            provider,
            verifying_key,
            config,
        }
    }

    pub fn activate(
        &self,
        activation_path: &Path,
        raw_key: &str,
        device_hash: &str,
        now: DateTime<Utc>,
    ) -> Result<LicenseSnapshot, LicenseError> {
        let normalized = normalize_key(raw_key)?;
        let response = self.provider.activate(&normalized, device_hash)?;
        self.accept_server_response(activation_path, response, device_hash, now)
    }

    pub fn status(
        &self,
        activation_path: &Path,
        device_hash: &str,
        now: DateTime<Utc>,
        force_online: bool,
    ) -> LicenseSnapshot {
        let mut stored = match read_local_activation(activation_path) {
            Ok(Some(value)) => value,
            Ok(None) => return LicenseSnapshot::unlicensed(),
            Err(_) => {
                return LicenseSnapshot::blocked(
                    DesktopLicenseState::INVALID_TOKEN,
                    None,
                    None,
                    "Yerel aktivasyon kaydı bozuk veya geçersiz.",
                )
            }
        };
        let claims = match verify_token_signature(&stored.token, &self.verifying_key) {
            Ok(value) => value,
            Err(error) => {
                return LicenseSnapshot::blocked(
                    error
                        .authoritative_state()
                        .unwrap_or(DesktopLicenseState::INVALID_TOKEN),
                    None,
                    Some(&stored),
                    error.to_string(),
                )
            }
        };
        let clock_rollback = clock_rollback_detected(&stored, now, self.config);
        let validation_time = if clock_rollback {
            trusted_clock(&stored).unwrap_or(now)
        } else {
            now
        };
        if let Err(error) = validate_claims(&claims, device_hash, validation_time, self.config) {
            return LicenseSnapshot::blocked(
                error
                    .authoritative_state()
                    .unwrap_or(DesktopLicenseState::INVALID_TOKEN),
                Some(claims),
                Some(&stored),
                error.to_string(),
            );
        }

        let must_verify = force_online
            || clock_rollback
            || stored.authoritative_state.is_some()
            || online_revalidation_due(&claims, now);
        if !must_verify {
            stored.last_accepted_wall_clock = Some(now);
            let _ = write_local_activation(activation_path, &stored);
            return snapshot_from_claims(
                DesktopLicenseState::ACTIVE,
                claims,
                &stored,
                "Lisans yerel imza ile doğrulandı.",
            );
        }

        match self.provider.verify(&stored.token, device_hash) {
            Ok(response) => {
                match self.accept_server_response(activation_path, response, device_hash, now) {
                    Ok(snapshot) => snapshot,
                    Err(error) => LicenseSnapshot::blocked(
                        error
                            .authoritative_state()
                            .unwrap_or(DesktopLicenseState::INVALID_TOKEN),
                        Some(claims),
                        Some(&stored),
                        error.to_string(),
                    ),
                }
            }
            Err(error)
                if matches!(
                    error,
                    LicenseError::ServerUnavailable | LicenseError::Configuration
                ) =>
            {
                if let Some(authoritative_state) = stored.authoritative_state {
                    return LicenseSnapshot::blocked(
                        authoritative_state,
                        Some(claims),
                        Some(&stored),
                        "Son sunucu kararı geçerliliğini koruyor; çevrimiçi doğrulama gerekli.",
                    );
                }
                if clock_rollback {
                    return LicenseSnapshot::blocked(
                        DesktopLicenseState::CLOCK_ANOMALY,
                        Some(claims),
                        Some(&stored),
                        LicenseError::ClockAnomaly.to_string(),
                    );
                }
                if now <= claims.offline_grace_until {
                    stored.last_accepted_wall_clock = Some(now);
                    let _ = write_local_activation(activation_path, &stored);
                    snapshot_from_claims(
                        DesktopLicenseState::OFFLINE_GRACE,
                        claims,
                        &stored,
                        "Sunucuya ulaşılamıyor; imzalı çevrimdışı tolerans süresi kullanılıyor.",
                    )
                } else {
                    LicenseSnapshot::blocked(
                        DesktopLicenseState::SERVER_UNAVAILABLE,
                        Some(claims),
                        Some(&stored),
                        "Çevrimdışı tolerans süresi sona erdi; sunucu doğrulaması gerekli.",
                    )
                }
            }
            Err(error) => {
                if let Some(state) = error.authoritative_state() {
                    stored.authoritative_state = Some(state);
                    stored.last_accepted_wall_clock = Some(now);
                    let _ = write_local_activation(activation_path, &stored);
                }
                LicenseSnapshot::blocked(
                    error
                        .authoritative_state()
                        .unwrap_or(DesktopLicenseState::INVALID_TOKEN),
                    Some(claims),
                    Some(&stored),
                    error.to_string(),
                )
            }
        }
    }

    fn accept_server_response(
        &self,
        activation_path: &Path,
        response: ServerLicenseResponse,
        device_hash: &str,
        now: DateTime<Utc>,
    ) -> Result<LicenseSnapshot, LicenseError> {
        if !response.ok {
            return Err(LicenseError::InvalidToken);
        }
        let claims = verify_token_signature(&response.activation_token, &self.verifying_key)?;
        validate_claims(&claims, device_hash, response.server_time, self.config)?;
        if claims.license_id != response.license.id
            || claims.plan != response.license.plan
            || claims.status != response.license.status
            || claims.device_binding_version != response.license.device_binding_version
            || claims.license_expires_at != response.license.expires_at
            || claims.online_revalidate_after != response.online_revalidate_after
            || claims.offline_grace_until != response.offline_grace_until
            || !response.license.device_bound
        {
            return Err(LicenseError::InvalidToken);
        }
        let stored = StoredActivation {
            token: response.activation_token,
            activated_at: response.license.activated_at,
            last_verified_at: Some(response.server_time),
            last_successful_server_time: Some(response.server_time),
            last_accepted_wall_clock: Some(now.max(response.server_time)),
            authoritative_state: None,
        };
        write_local_activation(activation_path, &stored)?;
        Ok(snapshot_from_claims(
            DesktopLicenseState::ACTIVE,
            claims,
            &stored,
            "Lisans sunucu tarafından doğrulandı.",
        ))
    }
}

fn snapshot_from_claims(
    state: DesktopLicenseState,
    claims: LicenseClaims,
    stored: &StoredActivation,
    message: impl Into<String>,
) -> LicenseSnapshot {
    LicenseSnapshot {
        state,
        claims: Some(claims),
        activated_at: stored.activated_at,
        last_verified_at: stored.last_verified_at,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;

    #[derive(Clone)]
    struct MockProvider {
        activation: Arc<Mutex<Result<ServerLicenseResponse, LicenseError>>>,
        verification: Arc<Mutex<Result<ServerLicenseResponse, LicenseError>>>,
        verify_calls: Arc<Mutex<usize>>,
    }

    impl MockProvider {
        fn new(response: ServerLicenseResponse) -> Self {
            Self {
                activation: Arc::new(Mutex::new(Ok(response.clone()))),
                verification: Arc::new(Mutex::new(Ok(response))),
                verify_calls: Arc::new(Mutex::new(0)),
            }
        }

        fn set_verification(&self, value: Result<ServerLicenseResponse, LicenseError>) {
            *self.verification.lock().unwrap() = value;
        }
    }

    impl LicenseProvider for MockProvider {
        fn activate(&self, _: &str, _: &str) -> Result<ServerLicenseResponse, LicenseError> {
            self.activation.lock().unwrap().clone()
        }

        fn verify(&self, _: &str, _: &str) -> Result<ServerLicenseResponse, LicenseError> {
            *self.verify_calls.lock().unwrap() += 1;
            self.verification.lock().unwrap().clone()
        }
    }

    fn signing_pair() -> (SigningKey, VerifyingKey, String) {
        let signing = SigningKey::generate(&mut rand_core::OsRng);
        let verifying = signing.verifying_key();
        let mut spki = ED25519_SPKI_PREFIX.to_vec();
        spki.extend_from_slice(verifying.as_bytes());
        (signing, verifying, URL_SAFE_NO_PAD.encode(spki))
    }

    fn sign_claims(claims: &LicenseClaims, signing: &SigningKey) -> String {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"Ed25519","typ":"ARZ-LICENSE","v":1}"#);
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(claims).unwrap());
        let signature = signing.sign(format!("{header}.{payload}").as_bytes());
        format!(
            "{header}.{payload}.{}",
            URL_SAFE_NO_PAD.encode(signature.to_bytes())
        )
    }

    fn response(
        signing: &SigningKey,
        now: DateTime<Utc>,
        plan: LicensePlan,
        status: LicenseStatus,
        binding: i64,
    ) -> ServerLicenseResponse {
        let expires = (plan != LicensePlan::LIFETIME).then_some(now + Duration::days(30));
        let claims = LicenseClaims {
            license_id: "license-1".into(),
            plan,
            status,
            device_fingerprint_hash: "device-a".into(),
            device_binding_version: binding,
            issued_at: now,
            license_expires_at: expires,
            online_revalidate_after: now + Duration::days(1),
            offline_grace_until: now + Duration::days(3),
            token_version: TOKEN_VERSION,
        };
        ServerLicenseResponse {
            ok: true,
            license: ServerLicense {
                id: claims.license_id.clone(),
                plan,
                status,
                activated_at: Some(now),
                expires_at: expires,
                device_bound: true,
                device_binding_version: binding,
                last_verified_at: Some(now),
            },
            activation_token: sign_claims(&claims, signing),
            server_time: now,
            online_revalidate_after: claims.online_revalidate_after,
            offline_grace_until: claims.offline_grace_until,
        }
    }

    fn setup(
        now: DateTime<Utc>,
        plan: LicensePlan,
    ) -> (TempDir, LicenseManager<MockProvider>, MockProvider) {
        let (signing, verifying, spki) = signing_pair();
        assert_eq!(verifying_key_from_spki_b64(&spki).unwrap(), verifying);
        let provider = MockProvider::new(response(&signing, now, plan, LicenseStatus::ACTIVE, 0));
        let manager = LicenseManager::new(provider.clone(), verifying, LicenseConfig::default());
        (tempfile::tempdir().unwrap(), manager, provider)
    }

    fn activation_path(temp: &TempDir) -> std::path::PathBuf {
        temp.path().join("activation.json")
    }

    #[test]
    fn normalization_hashing_and_spki_contract() {
        let key = normalize_key(" arzp-aaaa-bbbb-cccc-dddd ").unwrap();
        assert_eq!(key, "ARZP-AAAA-BBBB-CCCC-DDDD");
        assert_eq!(hash_license_key(&key).len(), 64);
        assert_eq!(
            normalize_key("ARZP-AAAA-BBBB-CCCC-DDD!"),
            Err(LicenseError::InvalidKey)
        );
    }

    #[test]
    fn valid_token_activation_and_restart_reload() {
        let now = Utc::now();
        let (temp, manager, provider) = setup(now, LicensePlan::MONTHLY);
        let path = activation_path(&temp);
        let activated = manager
            .activate(&path, "ARZP-AAAA-BBBB-CCCC-DDDD", "device-a", now)
            .unwrap();
        assert_eq!(activated.state, DesktopLicenseState::ACTIVE);
        let restarted =
            LicenseManager::new(provider, manager.verifying_key, LicenseConfig::default());
        assert_eq!(
            restarted
                .status(&path, "device-a", now + Duration::minutes(1), false)
                .state,
            DesktopLicenseState::ACTIVE
        );
    }

    #[test]
    fn tampered_and_corrupt_tokens_fail_closed() {
        let now = Utc::now();
        let (temp, manager, _) = setup(now, LicensePlan::MONTHLY);
        let path = activation_path(&temp);
        manager
            .activate(&path, "ARZP-AAAA-BBBB-CCCC-DDDD", "device-a", now)
            .unwrap();
        let mut stored = read_local_activation(&path).unwrap().unwrap();
        stored.token.push('x');
        write_local_activation(&path, &stored).unwrap();
        assert_eq!(
            manager.status(&path, "device-a", now, false).state,
            DesktopLicenseState::INVALID_TOKEN
        );
        fs::write(&path, b"not-json").unwrap();
        assert_eq!(
            manager.status(&path, "device-a", now, false).state,
            DesktopLicenseState::INVALID_TOKEN
        );
    }

    #[test]
    fn wrong_device_and_expired_finite_license_are_blocked() {
        let now = Utc::now();
        let (temp, manager, _) = setup(now, LicensePlan::MONTHLY);
        let path = activation_path(&temp);
        manager
            .activate(&path, "ARZP-AAAA-BBBB-CCCC-DDDD", "device-a", now)
            .unwrap();
        assert_eq!(
            manager.status(&path, "device-b", now, false).state,
            DesktopLicenseState::DEVICE_MISMATCH
        );
        assert_eq!(
            manager
                .status(&path, "device-a", now + Duration::days(31), false)
                .state,
            DesktopLicenseState::EXPIRED
        );
    }

    #[test]
    fn lifetime_has_no_fake_expiry() {
        let now = Utc::now();
        let (temp, manager, _) = setup(now, LicensePlan::LIFETIME);
        let snapshot = manager
            .activate(
                &activation_path(&temp),
                "ARZP-AAAA-BBBB-CCCC-DDDD",
                "device-a",
                now,
            )
            .unwrap();
        assert_eq!(snapshot.claims.unwrap().license_expires_at, None);
    }

    #[test]
    fn online_revalidation_is_skipped_before_due_and_called_when_due() {
        let now = Utc::now();
        let (temp, manager, provider) = setup(now, LicensePlan::MONTHLY);
        let path = activation_path(&temp);
        manager
            .activate(&path, "ARZP-AAAA-BBBB-CCCC-DDDD", "device-a", now)
            .unwrap();
        manager.status(&path, "device-a", now + Duration::hours(2), false);
        assert_eq!(*provider.verify_calls.lock().unwrap(), 0);
        manager.status(&path, "device-a", now + Duration::days(1), false);
        assert_eq!(*provider.verify_calls.lock().unwrap(), 1);
    }

    #[test]
    fn server_unavailable_uses_only_signed_grace_window() {
        let now = Utc::now();
        let (temp, manager, provider) = setup(now, LicensePlan::MONTHLY);
        let path = activation_path(&temp);
        manager
            .activate(&path, "ARZP-AAAA-BBBB-CCCC-DDDD", "device-a", now)
            .unwrap();
        provider.set_verification(Err(LicenseError::ServerUnavailable));
        assert_eq!(
            manager
                .status(&path, "device-a", now + Duration::days(1), false)
                .state,
            DesktopLicenseState::OFFLINE_GRACE
        );
        assert_eq!(
            manager
                .status(&path, "device-a", now + Duration::days(4), false)
                .state,
            DesktopLicenseState::SERVER_UNAVAILABLE
        );
    }

    #[test]
    fn server_unavailable_before_revalidation_keeps_active_state() {
        let now = Utc::now();
        let (temp, manager, provider) = setup(now, LicensePlan::MONTHLY);
        let path = activation_path(&temp);
        manager
            .activate(&path, "ARZP-AAAA-BBBB-CCCC-DDDD", "device-a", now)
            .unwrap();
        provider.set_verification(Err(LicenseError::ServerUnavailable));
        assert_eq!(
            manager
                .status(&path, "device-a", now + Duration::hours(2), false)
                .state,
            DesktopLicenseState::ACTIVE
        );
        assert_eq!(*provider.verify_calls.lock().unwrap(), 0);
    }

    #[test]
    fn clock_rollback_requires_online_and_never_extends_grace() {
        let now = Utc::now();
        let (temp, manager, provider) = setup(now, LicensePlan::MONTHLY);
        let path = activation_path(&temp);
        manager
            .activate(&path, "ARZP-AAAA-BBBB-CCCC-DDDD", "device-a", now)
            .unwrap();
        provider.set_verification(Err(LicenseError::ServerUnavailable));
        assert_eq!(
            manager
                .status(&path, "device-a", now - Duration::hours(1), false)
                .state,
            DesktopLicenseState::CLOCK_ANOMALY
        );
    }

    #[test]
    fn small_clock_drift_does_not_brick_a_valid_license() {
        let now = Utc::now();
        let (temp, manager, provider) = setup(now, LicensePlan::MONTHLY);
        let path = activation_path(&temp);
        manager
            .activate(&path, "ARZP-AAAA-BBBB-CCCC-DDDD", "device-a", now)
            .unwrap();
        provider.set_verification(Err(LicenseError::ServerUnavailable));
        assert_eq!(
            manager
                .status(&path, "device-a", now - Duration::minutes(4), false)
                .state,
            DesktopLicenseState::ACTIVE
        );
    }

    #[test]
    fn authoritative_suspend_and_revoke_never_fall_back_to_grace() {
        let now = Utc::now();
        let (temp, manager, provider) = setup(now, LicensePlan::YEARLY);
        let path = activation_path(&temp);
        manager
            .activate(&path, "ARZP-AAAA-BBBB-CCCC-DDDD", "device-a", now)
            .unwrap();
        provider.set_verification(Err(LicenseError::Suspended));
        assert_eq!(
            manager.status(&path, "device-a", now, true).state,
            DesktopLicenseState::SUSPENDED
        );
        provider.set_verification(Err(LicenseError::ServerUnavailable));
        assert_eq!(
            manager
                .status(&path, "device-a", now + Duration::minutes(1), false)
                .state,
            DesktopLicenseState::SUSPENDED
        );
        provider.set_verification(Err(LicenseError::Revoked));
        assert_eq!(
            manager
                .status(&path, "device-a", now + Duration::minutes(2), true)
                .state,
            DesktopLicenseState::REVOKED
        );
    }

    #[test]
    fn successful_reactivation_clears_authoritative_block() {
        let now = Utc::now();
        let (temp, manager, provider) = setup(now, LicensePlan::YEARLY);
        let path = activation_path(&temp);
        manager
            .activate(&path, "ARZP-AAAA-BBBB-CCCC-DDDD", "device-a", now)
            .unwrap();
        provider.set_verification(Err(LicenseError::Suspended));
        manager.status(&path, "device-a", now, true);
        provider.set_verification(provider.activation.lock().unwrap().clone());
        assert_eq!(
            manager
                .status(&path, "device-a", now + Duration::minutes(1), true)
                .state,
            DesktopLicenseState::ACTIVE
        );
        assert_eq!(
            read_local_activation(&path)
                .unwrap()
                .unwrap()
                .authoritative_state,
            None
        );
    }

    #[test]
    fn binding_version_refresh_replaces_old_token_and_rejection_blocks_it() {
        let now = Utc::now();
        let (signing, verifying, _) = signing_pair();
        let old = response(
            &signing,
            now,
            LicensePlan::MONTHLY,
            LicenseStatus::ACTIVE,
            0,
        );
        let provider = MockProvider::new(old);
        let manager = LicenseManager::new(provider.clone(), verifying, LicenseConfig::default());
        let temp = tempfile::tempdir().unwrap();
        let path = activation_path(&temp);
        manager
            .activate(&path, "ARZP-AAAA-BBBB-CCCC-DDDD", "device-a", now)
            .unwrap();
        provider.set_verification(Ok(response(
            &signing,
            now + Duration::minutes(1),
            LicensePlan::MONTHLY,
            LicenseStatus::ACTIVE,
            1,
        )));
        let refreshed = manager.status(&path, "device-a", now + Duration::minutes(1), true);
        assert_eq!(refreshed.claims.unwrap().device_binding_version, 1);
        provider.set_verification(Err(LicenseError::InvalidToken));
        assert_eq!(
            manager
                .status(&path, "device-a", now + Duration::minutes(2), true)
                .state,
            DesktopLicenseState::INVALID_TOKEN
        );
    }

    #[test]
    fn local_file_never_contains_raw_activation_key() {
        let now = Utc::now();
        let (temp, manager, _) = setup(now, LicensePlan::MONTHLY);
        let path = activation_path(&temp);
        let raw = "ARZP-AAAA-BBBB-CCCC-DDDD";
        manager.activate(&path, raw, "device-a", now).unwrap();
        assert!(!fs::read_to_string(path).unwrap().contains(raw));
    }
}
