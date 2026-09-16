use async_trait::async_trait;
use backend_flow_sdk::flow::StepRef;
use backend_flow_sdk::step::ContextUpdates;
use backend_flow_sdk::{Actor, FlowError, Step, StepContext, StepOutcome};
use chrono::Utc;
use rand::RngExt;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

/// Memory-hard, iterated KDF (Argon2id) with an explicit salt.
///
/// Phone hashes use a FIXED application salt so they stay deterministic and
/// re-derivable for case lookup (`get_case_by_phone_hash`) — they are never
/// projected to clients. OTP hashes use a per-issue RANDOM salt (stored next to
/// the hash) so the same OTP never hashes identically across issuances and an
/// attacker who reads a projected value cannot brute-force the 6-digit code
/// offline (review item C1).
fn kdf_secret(secret: &str, salt: &[u8]) -> String {
    use argon2::{Algorithm, Argon2, Params, Version};
    let params = Params::new(19 * 1024, 3, 1, Some(32)).expect("valid argon2 params");
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut out = [0u8; 32];
    let _ = argon2.hash_password_into(secret.as_bytes(), salt, &mut out);
    hex::encode(out)
}

const PHONE_HASH_SALT: &[u8] = b"azamra-recovery-phone-kdf-v1";

pub fn steps() -> Vec<StepRef> {
    vec![
        Arc::new(ResolveExistingAccountStep),
        Arc::new(IssueRecoveryOtpStep::new()),
        Arc::new(VerifyRecoveryOtpStep),
    ]
}

fn parse_config<T: serde::de::DeserializeOwned + Default>(
    ctx: &StepContext,
) -> Result<T, FlowError> {
    let val = ctx
        .services
        .config
        .as_ref()
        .map(|c| serde_json::to_value(c).unwrap_or_default())
        .unwrap_or_default();

    if val.is_null() || val.as_object().map(|o| o.is_empty()).unwrap_or(true) {
        Ok(T::default())
    } else {
        serde_json::from_value(val).map_err(|e| FlowError::InvalidDefinition(e.to_string()))
    }
}

/// Enumeration-safe mask of an E.164 phone number (keeps country + last 4).
pub(crate) fn mask_phone(phone: &str) -> String {
    let digits: String = phone.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() <= 4 {
        return "+****".to_string();
    }
    let (head, tail) = digits.split_at(digits.len() - 4);
    let prefix = if phone.starts_with('+') { "+" } else { "" };
    format!("{}{}****{}", prefix, &head[..head.len().min(4)], tail)
}

/// Deterministic phone hash (fixed salt) used for `get_case_by_phone_hash`
/// lookup. Never projected to clients (see `redact_recovery_fields`).
pub(crate) fn hash_phone(phone: &str) -> String {
    kdf_secret(phone, PHONE_HASH_SALT)
}

/// Per-issue salted OTP hash. `salt` must be the random per-issuance salt that
/// was stored alongside the hash.
fn hash_otp(otp: &str, salt: &[u8]) -> String {
    kdf_secret(otp, salt)
}

/// Generates a fresh random salt for one OTP issuance.
fn generate_otp_salt() -> String {
    let mut salt = [0u8; 16];
    rand::rng().fill(&mut salt);
    hex::encode(salt)
}

/// Constant-time string comparison to avoid leaking a mismatched OTP prefix
/// through early-exit timing.
fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn is_e164(value: &str) -> bool {
    match value.strip_prefix('+') {
        Some(digits) => {
            (8..=15).contains(&digits.len())
                && !digits.starts_with('0')
                && digits.chars().all(|c| c.is_ascii_digit())
        }
        None => false,
    }
}

/// Length cap avoids `10u64.pow(length)` overflowing/panicking for large
/// config values while keeping codes human-readable.
const MAX_OTP_LENGTH: u8 = 12;

fn generate_otp(length: u8) -> String {
    let length = length.clamp(1, MAX_OTP_LENGTH);
    let mut rng = rand::rng();
    let max = 10u64.pow(length as u32);
    let num = rng.random_range(0..max);
    format!("{:0width$}", num, width = length as usize)
}

fn now_ts() -> i64 {
    Utc::now().timestamp()
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ResolveConfig {
    #[serde(default)]
    phone_source: Option<String>,
    #[serde(default)]
    realm: Option<String>,
}

/// Resolves whether the requested phone maps to exactly one existing account,
/// without leaking account existence for unknown numbers.
pub struct ResolveExistingAccountStep;

#[async_trait]
impl Step for ResolveExistingAccountStep {
    fn step_type(&self) -> &'static str {
        "RESOLVE_EXISTING_ACCOUNT"
    }

    fn actor(&self) -> Actor {
        Actor::System
    }

    fn human_id(&self) -> &'static str {
        "resolve_existing_account"
    }

    fn feature(&self) -> Option<&'static str> {
        Some("flow-account-recovery")
    }

    async fn execute(&self, ctx: &StepContext) -> Result<StepOutcome, FlowError> {
        let config: ResolveConfig = parse_config(ctx)?;

        let phone = ctx
            .input
            .get("phone_number")
            .and_then(Value::as_str)
            .or_else(|| {
                ctx.session_context
                    .get("phone_number")
                    .and_then(Value::as_str)
            })
            .ok_or_else(|| FlowError::InvalidDefinition("Missing phone_number".to_string()))?
            .trim()
            .to_string();

        if !is_e164(&phone) {
            return Ok(StepOutcome::Failed {
                error: "INVALID_PHONE".to_string(),
                retryable: false,
            });
        }

        // C3(c): the lookup realm must match the configured recovery lookup
        // realm. The authoritative configured realm is injected into the
        // session context (`recovery.lookup_realm`) by the service layer at
        // session creation; a client-supplied realm that contradicts it is
        // rejected so cross-realm account-existence probing fails.
        let configured_realm = ctx
            .session_context
            .pointer("/recovery/lookup_realm")
            .and_then(Value::as_str);
        let requested_realm = config.realm.as_deref().or_else(|| {
            ctx.session_context
                .get("realm")
                .and_then(Value::as_str)
        });

        let realm = match configured_realm {
            Some(configured) => {
                if let Some(requested) = requested_realm
                    && requested != configured
                {
                    return Ok(StepOutcome::Failed {
                        error: "RECOVERY_REALM_REJECTED".to_string(),
                        retryable: false,
                    });
                }
                configured.to_string()
            }
            None => requested_realm.map(str::to_string).unwrap_or_default(),
        };

        let service = ctx.services.user_lookup.as_ref().ok_or_else(|| {
            FlowError::InvalidDefinition(
                "RESOLVE_EXISTING_ACCOUNT requires user lookup service".to_owned(),
            )
        })?;

        let candidates = service
            .find_users_by_phone(Some(realm), &phone)
            .await
            .map_err(FlowError::InvalidDefinition)?;

        let masked = mask_phone(&phone);
        let hash = hash_phone(&phone);

        let mut flow_patch = json!({
            "recovery": {
                "requested_phone_masked": masked,
                "requested_phone_hash": hash,
            }
        });

        let matched = candidates.len() == 1;
        if matched {
            let candidate = &candidates[0];
            flow_patch["recovery"]["matched"] = json!(true);
            flow_patch["recovery"]["matched_user_id"] = json!(candidate.user_id);
            flow_patch["recovery"]["phone_relation"] = json!("MATCHED");
        } else {
            flow_patch["recovery"]["matched"] = json!(false);
            flow_patch["recovery"]["phone_relation"] = json!(if candidates.is_empty() {
                "UNKNOWN"
            } else {
                "AMBIGUOUS"
            });
        }

        let updates = ContextUpdates {
            flow_context_patch: Some(flow_patch),
            session_context_patch: Some(json!({
                "requested_phone_masked": masked,
                "requested_phone_hash": hash,
            })),
            ..Default::default()
        };

        let branch = if matched { "matched" } else { "no_match" };

        Ok(StepOutcome::Branched {
            branch: branch.to_string(),
            output: Some(json!({
                "matched": matched,
                "requested_phone_masked": masked,
                "requested_phone_hash": hash,
                "matched_user_id": if matched {
                    candidates[0].user_id.clone().into()
                } else {
                    Value::Null
                }
            })),
            updates: Some(Box::new(updates)),
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
struct IssueOtpConfig {
    #[serde(default = "default_length")]
    length: u8,
    #[serde(default = "default_expiry")]
    expiry_seconds: u64,
    #[serde(default = "default_resend_cooldown")]
    resend_cooldown_seconds: u64,
    #[serde(default = "default_max_attempts")]
    max_attempts: u8,
}

fn default_length() -> u8 {
    6
}

fn default_expiry() -> u64 {
    1800
}

fn default_resend_cooldown() -> u64 {
    30
}

fn default_max_attempts() -> u8 {
    5
}

impl Default for IssueOtpConfig {
    fn default() -> Self {
        Self {
            length: default_length(),
            expiry_seconds: default_expiry(),
            resend_cooldown_seconds: default_resend_cooldown(),
            max_attempts: default_max_attempts(),
        }
    }
}

/// Payload sent to the sms-gateway `POST {base}/otp` endpoint.
#[derive(Debug, Clone)]
struct OtpDeliveryRequest {
    base_url: String,
    msisdn: String,
    otp: String,
    step_id: String,
}

/// Bounded delivery of a recovery OTP to the sms-gateway HTTP endpoint.
///
/// The plaintext code is carried only inside the outbound request body; it is
/// never returned in step output nor persisted anywhere.
#[async_trait]
trait OtpDeliverer: Send + Sync {
    async fn deliver(&self, req: &OtpDeliveryRequest) -> Result<bool, String>;
}

/// Real delivery: POSTs `${base_url}/otp` with `msisdn`/`otp`/`step_id` and
/// treats the OTP as sent only when the gateway returns `delivered: true`.
struct HttpOtpDeliverer {
    client: reqwest::Client,
    timeout: Duration,
}

impl Default for HttpOtpDeliverer {
    fn default() -> Self {
        Self {
            client: reqwest::Client::new(),
            timeout: Duration::from_secs(5),
        }
    }
}

#[async_trait]
impl OtpDeliverer for HttpOtpDeliverer {
    async fn deliver(&self, req: &OtpDeliveryRequest) -> Result<bool, String> {
        let url = format!("{}/otp", req.base_url.trim_end_matches('/'));
        let resp = self
            .client
            .post(&url)
            .timeout(self.timeout)
            .json(&json!({
                "msisdn": req.msisdn,
                "otp": req.otp,
                "step_id": req.step_id,
            }))
            .send()
            .await
            .map_err(|e| format!("OTP_DELIVERY_FAILED: {e}"))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(format!("OTP_DELIVERY_FAILED: http_{}", status.as_u16()));
        }

        let body: Value = resp
            .json()
            .await
            .map_err(|e| format!("OTP_DELIVERY_FAILED: bad_response: {e}"))?;
        Ok(body
            .get("delivered")
            .and_then(Value::as_bool)
            .unwrap_or(false))
    }
}

/// Issues a recovery OTP, storing only its salted KDF hash in flow context and
/// delivering the plaintext code to the sms-gateway HTTP endpoint. The
/// plaintext code is never returned in the step output and never persisted.
///
/// The deliverer is injectable so unit tests can exercise success/failure
/// without a live sms-gateway.
pub struct IssueRecoveryOtpStep {
    deliverer: Arc<dyn OtpDeliverer>,
    base_url: Option<String>,
}

impl Default for IssueRecoveryOtpStep {
    fn default() -> Self {
        Self::new()
    }
}

impl IssueRecoveryOtpStep {
    pub fn new() -> Self {
        Self {
            deliverer: Arc::new(HttpOtpDeliverer::default()),
            base_url: None,
        }
    }

    #[cfg(test)]
    fn with(deliverer: Arc<dyn OtpDeliverer>, base_url: Option<String>) -> Self {
        Self {
            deliverer,
            base_url,
        }
    }
}

#[async_trait]
impl Step for IssueRecoveryOtpStep {
    fn step_type(&self) -> &'static str {
        "ISSUE_RECOVERY_OTP"
    }

    fn actor(&self) -> Actor {
        Actor::System
    }

    fn human_id(&self) -> &'static str {
        "issue_recovery_otp"
    }

    fn feature(&self) -> Option<&'static str> {
        Some("flow-account-recovery")
    }

    async fn execute(&self, ctx: &StepContext) -> Result<StepOutcome, FlowError> {
        let config: IssueOtpConfig = parse_config(ctx)?;

        let matched = ctx
            .flow_config("recovery")
            .and_then(|r| r.get("matched"))
            .and_then(Value::as_bool)
            .unwrap_or(false);

        if !matched {
            // Enumeration-safe: no account to send OTP to; do not generate.
            return Ok(StepOutcome::Done {
                output: Some(json!({ "otp_sent": false, "reason": "NO_MATCH" })),
                updates: None,
            });
        }

        let now = now_ts();
        let resend_at = ctx
            .flow_config("recovery")
            .and_then(|r| r.get("otp_resend_at"))
            .and_then(Value::as_i64);
        let has_hash = ctx
            .flow_config("recovery")
            .and_then(|r| r.get("otp_hash"))
            .and_then(Value::as_str)
            .is_some();

        if has_hash
            && let Some(resend_at) = resend_at
            && now < resend_at
        {
            let remaining = resend_at - now;
            return Ok(StepOutcome::Branched {
                branch: "cooldown".to_string(),
                output: Some(json!({
                    "otp_sent": false,
                    "cooldown_remaining_seconds": remaining,
                })),
                updates: None,
            });
        }

        let otp = generate_otp(config.length);
        let salt = generate_otp_salt();
        let otp_hash = hash_otp(&otp, salt.as_bytes());
        let expires_at = now + config.expiry_seconds as i64;

        let mut recovery_patch = json!({
            "otp_hash": otp_hash,
            "otp_salt": salt,
            "otp_expires_at": expires_at,
            "otp_resend_at": now + config.resend_cooldown_seconds as i64,
            "otp_max_attempts": config.max_attempts,
            "otp_issued": true,
        });
        // M1: a resend must NOT reset the attempts counter, otherwise a locked
        // case could be unlocked by resending. Only the first issuance starts
        // the counter at zero.
        if !has_hash {
            recovery_patch["otp_attempts"] = json!(0);
        }

        let updates = ContextUpdates {
            flow_context_patch: Some(json!({ "recovery": recovery_patch })),
            ..Default::default()
        };

        let phone = ctx
            .session_context
            .get("phone_number")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        if phone.is_empty() {
            // No deliverable destination: do NOT claim the OTP was sent.
            return Ok(StepOutcome::Failed {
                error: "NO_DELIVERY_TARGET".to_string(),
                retryable: false,
            });
        }

        let base_url = self
            .base_url
            .clone()
            .or_else(|| std::env::var("SMS_SINK_URL").ok().filter(|s| !s.is_empty()));
        let Some(base_url) = base_url else {
            // sms-gateway base URL unavailable: cannot deliver, so never
            // report the OTP as sent.
            return Ok(StepOutcome::Failed {
                error: "OTP_DELIVERY_FAILED".to_string(),
                retryable: false,
            });
        };

        let request = OtpDeliveryRequest {
            base_url,
            msisdn: phone,
            otp,
            step_id: ctx.step_id.clone(),
        };

        let delivered = self.deliverer.deliver(&request).await;
        let delivered = match delivered {
            Ok(true) => true,
            Ok(false) => {
                // Gateway responded but did not confirm delivery.
                return Ok(StepOutcome::Failed {
                    error: "OTP_DELIVERY_FAILED".to_string(),
                    retryable: true,
                });
            }
            Err(e) => {
                return Ok(StepOutcome::Failed {
                    error: e,
                    retryable: true,
                });
            }
        };

        Ok(StepOutcome::Done {
            output: Some(json!({
                "otp_sent": delivered,
                "otp_issued": true,
                "expires_at": expires_at,
            })),
            updates: Some(Box::new(updates)),
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
struct VerifyOtpConfig {
    #[serde(default = "default_input_field")]
    input_field: String,
    #[serde(default = "default_max_attempts")]
    max_attempts: u8,
    #[serde(default = "default_attempts_field")]
    attempts_field: String,
}

fn default_input_field() -> String {
    "code".to_string()
}

fn default_attempts_field() -> String {
    "otp_attempts".to_string()
}

impl Default for VerifyOtpConfig {
    fn default() -> Self {
        Self {
            input_field: default_input_field(),
            max_attempts: default_max_attempts(),
            attempts_field: default_attempts_field(),
        }
    }
}

/// Verifies a submitted recovery OTP against the stored hash, enforcing the
/// 30-minute expiry and a max-tries lockout.
pub struct VerifyRecoveryOtpStep;

#[async_trait]
impl Step for VerifyRecoveryOtpStep {
    fn step_type(&self) -> &'static str {
        "VERIFY_RECOVERY_OTP"
    }

    fn actor(&self) -> Actor {
        Actor::EndUser
    }

    fn human_id(&self) -> &'static str {
        "verify_recovery_otp"
    }

    fn feature(&self) -> Option<&'static str> {
        Some("flow-account-recovery")
    }

    async fn execute(&self, _ctx: &StepContext) -> Result<StepOutcome, FlowError> {
        Ok(StepOutcome::Waiting {
            actor: Actor::EndUser,
        })
    }

    async fn validate_input(&self, input: &Value) -> Result<(), FlowError> {
        if input.get("code").is_none() {
            return Err(FlowError::InvalidDefinition(
                "Missing required field: code".to_owned(),
            ));
        }
        Ok(())
    }

    async fn verify_input(
        &self,
        ctx: &StepContext,
        input: &Value,
    ) -> Result<StepOutcome, FlowError> {
        let config: VerifyOtpConfig = parse_config(ctx)?;

        let submitted = input
            .get(&config.input_field)
            .and_then(Value::as_str)
            .ok_or_else(|| FlowError::InvalidDefinition("Missing code".to_string()))?;

        let recovery = ctx.flow_config("recovery");
        let stored_hash = recovery
            .and_then(|r| r.get("otp_hash"))
            .and_then(Value::as_str);
        let stored_salt = recovery
            .and_then(|r| r.get("otp_salt"))
            .and_then(Value::as_str);
        let expires_at = recovery
            .and_then(|r| r.get("otp_expires_at"))
            .and_then(Value::as_i64);
        let attempts_key = &config.attempts_field;
        let current_attempts: u8 = recovery
            .and_then(|r| r.get(attempts_key))
            .and_then(Value::as_u64)
            .map(|v| v as u8)
            .unwrap_or(0);

        let (Some(stored_hash), Some(expires_at)) = (stored_hash, expires_at) else {
            return Ok(StepOutcome::Failed {
                error: "NO_OTP_FOUND".to_string(),
                retryable: false,
            });
        };

        if now_ts() > expires_at {
            return Ok(StepOutcome::Failed {
                error: "OTP_EXPIRED".to_string(),
                retryable: false,
            });
        }

        // M1: a locked case is rejected before any hash work is done (skips
        // expensive KDF once the attempts are exhausted).
        if current_attempts >= config.max_attempts {
            return Ok(StepOutcome::Failed {
                error: "MAX_ATTEMPTS_EXCEEDED".to_string(),
                retryable: false,
            });
        }

        // A missing per-issue salt means the stored hash was produced by an
        // older (fixed-salt) issuance; treat it as unverifiable so we never
        // fall back to a deterministic comparison.
        let Some(stored_salt) = stored_salt else {
            return Ok(StepOutcome::Failed {
                error: "NO_OTP_FOUND".to_string(),
                retryable: false,
            });
        };

        let submitted_hash = hash_otp(submitted, stored_salt.as_bytes());
        if !constant_time_eq(stored_hash, &submitted_hash) {
            let new_attempts = current_attempts + 1;
            return Ok(StepOutcome::Branched {
                branch: "retry".to_string(),
                output: Some(json!({
                    "verified": false,
                    "attempts_remaining": config.max_attempts.saturating_sub(new_attempts)
                })),
                updates: Some(Box::new(ContextUpdates {
                    flow_context_patch: Some(json!({
                        "recovery": { attempts_key: new_attempts }
                    })),
                    ..Default::default()
                })),
            });
        }

        Ok(StepOutcome::Branched {
            branch: "verified".to_string(),
            output: Some(json!({ "verified": true })),
            updates: Some(Box::new(ContextUpdates {
                flow_context_patch: Some(json!({
                    "recovery": {
                        "otp_hash": Value::Null,
                        "otp_expires_at": Value::Null,
                        attempts_key: Value::Null,
                        "otp_verified": true,
                    }
                })),
                ..Default::default()
            })),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use backend_flow_sdk::StepServices;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// A fixed 16-byte salt (hex) used only by unit tests.
    const TEST_SALT: &str = "cafebabecafebabecafebabecafebabe";

    struct FakeDeliverer {
        result: Mutex<Result<bool, String>>,
        requests: Arc<Mutex<Vec<OtpDeliveryRequest>>>,
    }

    #[async_trait]
    impl OtpDeliverer for FakeDeliverer {
        async fn deliver(&self, req: &OtpDeliveryRequest) -> Result<bool, String> {
            self.requests.lock().unwrap().push(req.clone());
            self.result.lock().unwrap().clone()
        }
    }

    fn step_with_deliverer(
        result: Result<bool, String>,
    ) -> (IssueRecoveryOtpStep, Arc<Mutex<Vec<OtpDeliveryRequest>>>) {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let deliverer = FakeDeliverer {
            result: Mutex::new(result),
            requests: requests.clone(),
        };
        let step = IssueRecoveryOtpStep::with(
            Arc::new(deliverer),
            Some("http://sms-gateway:3000".to_string()),
        );
        (step, requests)
    }

    fn make_ctx(
        session_context: Value,
        flow_context: Value,
        config: HashMap<String, Value>,
    ) -> StepContext {
        StepContext {
            session_id: "sess".to_string(),
            session_user_id: None,
            flow_id: "flow".to_string(),
            step_id: "step".to_string(),
            input: json!({}),
            session_context,
            flow_context,
            services: StepServices {
                config: if config.is_empty() {
                    None
                } else {
                    Some(config)
                },
                ..Default::default()
            },
        }
    }

    fn otp_config() -> HashMap<String, Value> {
        let mut c = HashMap::new();
        c.insert("length".to_string(), json!(6));
        c.insert("expiry_seconds".to_string(), json!(1800));
        c.insert("resend_cooldown_seconds".to_string(), json!(30));
        c.insert("max_attempts".to_string(), json!(5));
        c
    }

    fn matched_flow() -> Value {
        json!({ "recovery": { "matched": true } })
    }

    fn matched_session() -> Value {
        json!({ "phone_number": "+237690000000", "realm": "fineract" })
    }

    #[test]
    fn otp_hash_is_salted_per_issuance() {
        let otp = "123456";
        let salt_a = generate_otp_salt();
        let salt_b = generate_otp_salt();
        let h = hash_otp(otp, salt_a.as_bytes());
        assert_eq!(h.len(), 64);
        assert_ne!(h, otp);
        // The same OTP with the same salt is deterministic...
        assert_eq!(h, hash_otp(otp, salt_a.as_bytes()));
        // ...but with a different (fresh per-issue) salt it differs, defeating
        // offline brute-force of a projected hash.
        assert_ne!(h, hash_otp(otp, salt_b.as_bytes()));
        assert_ne!(salt_a, salt_b, "fresh salts must differ");
        assert_ne!(h, hash_otp("654321", salt_a.as_bytes()));
    }

    #[test]
    fn constant_time_compare_rejects_mismatch_and_matches_identically() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "abcd"));
        assert!(!constant_time_eq("", "x"));
    }

    #[test]
    fn generate_otp_clamps_length() {
        for length in [1u8, 6, 12, 64, 200] {
            let otp = generate_otp(length);
            assert!(otp.len() <= MAX_OTP_LENGTH as usize, "len {otp}");
            assert!(otp.bytes().all(|b| b.is_ascii_digit()));
        }
    }

    #[tokio::test]
    async fn issue_recovery_otp_delivers_via_http_not_persisted_output() {
        let (step, requests) = step_with_deliverer(Ok(true));
        let ctx = make_ctx(matched_session(), matched_flow(), otp_config());
        let outcome = step.execute(&ctx).await.unwrap();
        match outcome {
            StepOutcome::Done { output, updates } => {
                let output = output.unwrap();
                assert_eq!(output["otp_sent"], true);
                assert!(
                    output.get("otp").is_none(),
                    "plaintext must not be in output"
                );

                let updates = updates.unwrap();
                let patch = updates.flow_context_patch.unwrap();
                assert!(patch["recovery"]["otp_hash"].as_str().is_some());
                assert!(patch["recovery"]["otp_salt"].as_str().is_some());
                assert_eq!(patch["recovery"]["otp_attempts"], 0);
                assert!(patch["recovery"]["otp_expires_at"].is_i64());

                assert!(
                    updates.notifications.is_none(),
                    "no Redis notification should be enqueued"
                );

                let reqs = requests.lock().unwrap();
                assert_eq!(reqs.len(), 1, "exactly one HTTP delivery");
                let req = &reqs[0];
                assert_eq!(req.msisdn, "+237690000000");
                assert_eq!(req.step_id, "step");
                assert!(!req.otp.is_empty());

                let stored = patch["recovery"]["otp_hash"].as_str().unwrap();
                let salt = patch["recovery"]["otp_salt"].as_str().unwrap();
                assert_eq!(stored, hash_otp(&req.otp, salt.as_bytes()));

                let serialized_output = output.to_string();
                assert!(
                    !serialized_output.contains(&req.otp),
                    "plaintext OTP leaked into output: {serialized_output}"
                );
                let serialized_patch = patch.to_string();
                assert!(
                    !serialized_patch.contains(&req.otp),
                    "plaintext OTP leaked into flow context: {serialized_patch}"
                );
            }
            other => panic!("expected Done, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn issue_recovery_otp_fails_when_gateway_errors() {
        let (step, _) = step_with_deliverer(Err("OTP_DELIVERY_FAILED: network".to_string()));
        let ctx = make_ctx(matched_session(), matched_flow(), otp_config());
        match step.execute(&ctx).await.unwrap() {
            StepOutcome::Failed { error, retryable } => {
                assert!(
                    error.starts_with("OTP_DELIVERY_FAILED"),
                    "unexpected error: {error}"
                );
                assert!(retryable);
            }
            other => panic!("expected failed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn issue_recovery_otp_does_not_report_sent_when_gateway_denies_delivery() {
        let (step, _) = step_with_deliverer(Ok(false));
        let ctx = make_ctx(matched_session(), matched_flow(), otp_config());
        match step.execute(&ctx).await.unwrap() {
            StepOutcome::Failed { error, retryable } => {
                assert_eq!(error, "OTP_DELIVERY_FAILED");
                assert!(retryable);
            }
            other => panic!("expected failed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn issue_recovery_otp_fails_without_delivery_target() {
        let (step, _) = step_with_deliverer(Ok(true));
        let ctx = make_ctx(json!({}), matched_flow(), otp_config());
        match step.execute(&ctx).await.unwrap() {
            StepOutcome::Failed { error, .. } => assert_eq!(error, "NO_DELIVERY_TARGET"),
            other => panic!("expected failed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn issue_recovery_otp_enforces_resend_cooldown() {
        let (step, _) = step_with_deliverer(Ok(true));
        let flow_context = json!({
            "recovery": {
                "matched": true,
                "otp_hash": "existing",
                "otp_resend_at": now_ts() + 30,
            }
        });
        let ctx = make_ctx(json!({}), flow_context, otp_config());
        match step.execute(&ctx).await.unwrap() {
            StepOutcome::Branched { branch, output, .. } => {
                assert_eq!(branch, "cooldown");
                assert_eq!(output.unwrap()["otp_sent"], false);
            }
            other => panic!("expected cooldown branch, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn issue_recovery_otp_resend_does_not_reset_attempts() {
        let (step, _) = step_with_deliverer(Ok(true));
        // A previous issuance already exhausted attempts (case locked).
        let flow_context = json!({
            "recovery": {
                "matched": true,
                "otp_hash": "existing",
                "otp_attempts": 5,
                "otp_max_attempts": 5,
            }
        });
        let ctx = make_ctx(
            json!({ "phone_number": "+237690000000" }),
            flow_context,
            otp_config(),
        );
        match step.execute(&ctx).await.unwrap() {
            StepOutcome::Done { updates, .. } => {
                let patch = updates.unwrap().flow_context_patch.unwrap();
                // M1: a resend must NOT reset the attempts counter, so a locked
                // case stays locked. The patch simply omits otp_attempts, which
                // leaves the existing (exhausted) count in place.
                assert!(
                    !patch["recovery"]
                        .as_object()
                        .map(|o| o.contains_key("otp_attempts"))
                        .unwrap_or(true),
                    "resend must not touch otp_attempts: {patch}"
                );
            }
            other => panic!("expected done resend, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn verify_recovery_otp_accepts_correct_code() {
        let step = VerifyRecoveryOtpStep;
        let otp = "246810";
        let flow_context = json!({
            "recovery": {
                "otp_hash": hash_otp(otp, TEST_SALT.as_bytes()),
                "otp_salt": TEST_SALT,
                "otp_expires_at": now_ts() + 1800,
                "otp_attempts": 0,
            }
        });
        let ctx = make_ctx(json!({}), flow_context, otp_config());
        match step
            .verify_input(&ctx, &json!({ "code": otp }))
            .await
            .unwrap()
        {
            StepOutcome::Branched { branch, .. } => assert_eq!(branch, "verified"),
            other => panic!("expected verified, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn verify_recovery_otp_rejects_wrong_code_and_tracks_attempts() {
        let step = VerifyRecoveryOtpStep;
        let otp = "246810";
        let flow_context = json!({
            "recovery": {
                "otp_hash": hash_otp(otp, TEST_SALT.as_bytes()),
                "otp_salt": TEST_SALT,
                "otp_expires_at": now_ts() + 1800,
                "otp_attempts": 0,
            }
        });
        let ctx = make_ctx(json!({}), flow_context, otp_config());
        match step
            .verify_input(&ctx, &json!({ "code": "000000" }))
            .await
            .unwrap()
        {
            StepOutcome::Branched {
                branch, updates, ..
            } => {
                assert_eq!(branch, "retry");
                let patch = updates.unwrap().flow_context_patch.unwrap();
                assert_eq!(patch["recovery"]["otp_attempts"], 1);
            }
            other => panic!("expected retry, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn verify_recovery_otp_locks_out_after_max_attempts() {
        let step = VerifyRecoveryOtpStep;
        let otp = "246810";
        let flow_context = json!({
            "recovery": {
                "otp_hash": hash_otp(otp, TEST_SALT.as_bytes()),
                "otp_salt": TEST_SALT,
                "otp_expires_at": now_ts() + 1800,
                "otp_attempts": 5,
            }
        });
        let ctx = make_ctx(json!({}), flow_context, otp_config());
        match step
            .verify_input(&ctx, &json!({ "code": otp }))
            .await
            .unwrap()
        {
            StepOutcome::Failed { error, retryable } => {
                assert_eq!(error, "MAX_ATTEMPTS_EXCEEDED");
                assert!(!retryable);
            }
            other => panic!("expected lockout, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn verify_recovery_otp_fails_expired() {
        let step = VerifyRecoveryOtpStep;
        let otp = "246810";
        let flow_context = json!({
            "recovery": {
                "otp_hash": hash_otp(otp, TEST_SALT.as_bytes()),
                "otp_salt": TEST_SALT,
                "otp_expires_at": now_ts() - 10,
                "otp_attempts": 0,
            }
        });
        let ctx = make_ctx(json!({}), flow_context, otp_config());
        match step
            .verify_input(&ctx, &json!({ "code": otp }))
            .await
            .unwrap()
        {
            StepOutcome::Failed { error, .. } => assert_eq!(error, "OTP_EXPIRED"),
            other => panic!("expected expired, got {other:?}"),
        }
    }

    #[test]
    fn mask_phone_keeps_country_and_last_four() {
        assert_eq!(mask_phone("+237690000000"), "+2376****0000");
        assert_eq!(mask_phone("1234"), "+****");
    }
}
