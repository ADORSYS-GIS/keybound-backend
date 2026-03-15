# Flow SDK Architecture Revamp

**Status**: Planned
**Date**: 2025-03-15
**Affects**: `/bff`, `/auth`, CLI, Database Schema

## Overview

This document describes the architecture for revamping the KYC and account management flows with a three-level hierarchy (Sessions → Flows → Steps), a new SDK crate, and a dedicated `/auth` API surface for device-bound authentication.

### Key Changes

| Component | Current | Proposed |
|-----------|---------|----------|
| Session model | 2-level (session → steps) | 3-level (session → flows → steps) |
| IDs | CUID only | CUID + semantic human-readable paths |
| Feature gating | None | Cargo features per flow/step type |
| Import/export | N/A | CLI-based YAML/JSON declarative import/export |
| New API surface | N/A | `/auth` for enrollment, devices, approvals, tokens |
| Token exchange | N/A | Device signature → access token |
| JWKS | N/A | Backend-managed RSA keys for token validation |

---

## API Surfaces

| Surface | Purpose | Security |
|---------|---------|----------|
| `/auth` | Enrollment, devices, approvals, token exchange | Device signature (most); enrollment/JWKS/token exempt |
| `/bff` | KYC flows (revamped) | OAuth2 (external provider) |
| `/kc` | Keycloak integration | KC signature (shared secret) |
| `/staff` | Admin operations | OAuth2 (external provider) |

---

## New Crate: `backend-flow-sdk`

**Location**: `app/crates/backend-flow-sdk`

### Purpose

Provides traits, types, and utilities for defining and executing KYC/account flows. Enables:
- Type-safe step definitions
- Feature-gated flow registration
- Declarative import/export
- Actor-based execution routing

### Structure

```
backend-flow-sdk/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── actor.rs         # Actor enum (System, Admin, EndUser)
    ├── step.rs          # Step trait + outcomes
    ├── flow.rs          # Flow trait + definitions
    ├── session.rs       # Session definition
    ├── id.rs            # HumanReadableId (semantic paths)
    ├── registry.rs      # Global step/flow registry
    ├── import.rs        # YAML/JSON import
    ├── export.rs        # YAML/JSON export
    ├── context.rs       # Execution context
    └── error.rs
```

### Core Types

#### Actor

```rust
// actor.rs
pub enum Actor {
    System,    // Background worker executes automatically
    Admin,     // Staff approval required
    EndUser,   // User action required (OTP entry, etc.)
}
```

#### Step Trait

```rust
// step.rs
pub trait Step: Send + Sync + 'static {
    /// Unique identifier for this step type (e.g., "SEND_OTP")
    fn step_type(&self) -> &'static str;
    
    /// Who performs this step
    fn actor(&self) -> Actor;
    
    /// Human-readable ID suffix (e.g., "send")
    fn human_id(&self) -> &'static str;
    
    /// Feature flag that gates this step (None = always enabled)
    fn feature(&self) -> Option<&'static str> { None }
    
    /// Execute the step (for System actors)
    async fn execute(&self, ctx: &StepContext) -> Result<StepOutcome, FlowError>;
    
    /// Validate external input (for Admin/EndUser actors)
    async fn validate_input(&self, input: &Value) -> Result<(), FlowError> { Ok(()) }
}

pub enum StepOutcome {
    Done,
    Waiting { actor: Actor },
    Failed { error: String, retryable: bool },
    Retry { after: Duration },
}
```

#### Flow Trait

```rust
// flow.rs
pub trait Flow: Send + Sync + 'static {
    /// Unique identifier for this flow type (e.g., "PHONE_OTP")
    fn flow_type(&self) -> &'static str;
    
    /// Human-readable ID suffix (e.g., "phone_otp")
    fn human_id(&self) -> &'static str;
    
    /// Feature flag that gates this flow
    fn feature(&self) -> Option<&'static str>;
    
    /// Ordered list of steps in this flow
    fn steps(&self) -> &[Box<dyn Step>];
    
    /// Initial step type
    fn initial_step(&self) -> &'static str;
    
    /// Step transitions (on_success, on_failure)
    fn transitions(&self) -> &HashMap<String, StepTransition>;
}

pub struct StepTransition {
    pub on_success: String,
    pub on_failure: Option<String>,
}
```

#### Human-Readable IDs

```rust
// id.rs
pub struct HumanReadableId(String);

impl HumanReadableId {
    /// Creates from parts: ["kyc", "2026-03-15", "phone_otp", "send"]
    /// Result: "kyc.2026-03-15.phone_otp.send"
    pub fn new(parts: &[&str]) -> Self;
    
    /// Get parent ID (e.g., "kyc.2026-03-15.phone_otp")
    pub fn parent(&self) -> Option<Self>;
    
    /// Split into parts
    pub fn parts(&self) -> Vec<&str>;
    
    /// Add suffix
    pub fn with_suffix(&self, suffix: &str) -> Self;
}
```

#### Registry

```rust
// registry.rs
pub struct FlowRegistry {
    steps: HashMap<String, Box<dyn Step>>,
    flows: HashMap<String, Box<dyn Flow>>,
    sessions: HashMap<String, SessionDefinition>,
}

impl FlowRegistry {
    pub fn register_step(&mut self, step: Box<dyn Step>);
    pub fn register_flow(&mut self, flow: Box<dyn Flow>);
    pub fn register_session(&mut self, session: SessionDefinition);
    
    pub fn get_step(&self, step_type: &str) -> Option<&dyn Step>;
    pub fn get_flow(&self, flow_type: &str) -> Option<&dyn Flow>;
    
    /// Validate that all registered items have enabled features
    pub fn validate_features(&self, enabled_features: &[&str]) -> Result<(), FlowError>;
}
```

---

## Database Schema

### New Tables

```sql
-- Add metadata column to app_user for webhook-extracted data (migration)
ALTER TABLE app_user ADD COLUMN IF NOT EXISTS metadata JSONB NOT NULL DEFAULT '{}';

-- Session: top-level container for a user's KYC/account process
CREATE TABLE flow_session (
    id TEXT PRIMARY KEY,                    -- ses_* CUID
    human_id TEXT UNIQUE NOT NULL,          -- "kyc.2026-03-15"
    user_id TEXT REFERENCES app_user(user_id),
    session_type TEXT NOT NULL,             -- "KYC_FULL", "ACCOUNT_MANAGEMENT"
    status TEXT NOT NULL DEFAULT 'OPEN',    -- OPEN, RUNNING, COMPLETED, FAILED, CANCELLED
    context JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ
);

-- Flow: a sequential sequence within a session
CREATE TABLE flow_instance (
    id TEXT PRIMARY KEY,                    -- flw_* CUID
    human_id TEXT UNIQUE NOT NULL,          -- "kyc.2026-03-15.phone_otp"
    session_id TEXT NOT NULL REFERENCES flow_session(id) ON DELETE CASCADE,
    flow_type TEXT NOT NULL,                -- "PHONE_OTP", "EMAIL_MAGIC", "FIRST_DEPOSIT"
    status TEXT NOT NULL DEFAULT 'PENDING',
    current_step TEXT,                      -- Current step type
    step_ids JSONB NOT NULL DEFAULT '[]',   -- Ordered list of step internal IDs
    context JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Step: atomic action
CREATE TABLE flow_step (
    id TEXT PRIMARY KEY,                    -- stp_* CUID
    human_id TEXT UNIQUE NOT NULL,          -- "kyc.2026-03-15.phone_otp.send"
    flow_id TEXT NOT NULL REFERENCES flow_instance(id) ON DELETE CASCADE,
    step_type TEXT NOT NULL,                -- "SEND_OTP", "VERIFY_OTP"
    actor TEXT NOT NULL,                    -- SYSTEM, ADMIN, END_USER
    status TEXT NOT NULL DEFAULT 'PENDING',
    attempt_no INT NOT NULL DEFAULT 0,
    input JSONB,
    output JSONB,
    error JSONB,
    next_retry_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    finished_at TIMESTAMPTZ
);

-- JWKS signing keys
CREATE TABLE signing_key (
    kid TEXT PRIMARY KEY,                   -- Key ID
    private_key_pem TEXT NOT NULL,          -- RSA private key
    public_key_jwk JSONB NOT NULL,          -- JWK format for JWKS endpoint
    algorithm TEXT NOT NULL DEFAULT 'RS256',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,
    is_active BOOL NOT NULL DEFAULT TRUE
);

-- Indices
CREATE INDEX idx_flow_session_user ON flow_session(user_id);
CREATE INDEX idx_flow_instance_session ON flow_instance(session_id);
CREATE INDEX idx_flow_step_flow ON flow_step(flow_id);
CREATE INDEX idx_flow_step_status ON flow_step(status);
CREATE INDEX idx_signing_key_active ON signing_key(is_active) WHERE is_active = TRUE;

-- Add metadata column to app_user for webhook-extracted data
ALTER TABLE app_user ADD COLUMN IF NOT EXISTS metadata JSONB NOT NULL DEFAULT '{}';
CREATE INDEX idx_app_user_metadata ON app_user USING GIN (metadata);
```

### Migration from sm_* to flow_*

```sql
-- Migrate sm_instance -> flow_session
INSERT INTO flow_session (id, human_id, user_id, session_type, status, context, created_at, updated_at, completed_at)
SELECT 
    id,
    'legacy.' || id as human_id,
    user_id,
    kind as session_type,
    status,
    context,
    created_at,
    updated_at,
    completed_at
FROM sm_instance;

-- For each sm_instance, create flow_instance
INSERT INTO flow_instance (id, human_id, session_id, flow_type, status, step_ids, context, created_at, updated_at)
SELECT 
    'flw_' || id as id,
    'legacy.' || id || '.' || kind as human_id,
    id as session_id,
    kind as flow_type,
    status,
    context->'step_ids' as step_ids,
    context,
    created_at,
    updated_at
FROM sm_instance;

-- Migrate sm_step_attempt -> flow_step
INSERT INTO flow_step (id, human_id, flow_id, step_type, actor, status, attempt_no, input, output, error, next_retry_at, created_at, updated_at, finished_at)
SELECT 
    id,
    'legacy.' || instance_id || '.' || step_name as human_id,
    'flw_' || instance_id as flow_id,
    step_name as step_type,
    'SYSTEM' as actor,
    status,
    attempt_no,
    input,
    output,
    error,
    next_retry_at,
    queued_at as created_at,
    started_at as updated_at,
    finished_at
FROM sm_step_attempt;

-- After verification, drop old tables:
-- DROP TABLE sm_step_attempt;
-- DROP TABLE sm_event;
-- DROP TABLE sm_instance;
```

---

## New API Surface: `/auth`

### OpenAPI File

`openapi/user-auth.yaml`

### Endpoints

| Endpoint | Method | Auth | Description |
|----------|--------|------|-------------|
| `/auth/enroll` | POST | None | Start device enrollment (first device) |
| `/auth/enroll/{id}/bind` | POST | Signature | Bind device with public key |
| `/auth/devices` | GET | Signature | List user's devices |
| `/auth/devices/{id}` | DELETE | Signature | Revoke device |
| `/auth/token` | POST | Signature | Exchange signature for access token |
| `/auth/jwks` | GET | None | JWKS for token validation |
| `/auth/approve/{stepId}` | POST | Admin OAuth | Admin approval for waiting step |

### Token Endpoint

The token endpoint accepts a device signature and returns an access token for use with external OAuth2 systems.

**Request**:
```json
POST /auth/token
Headers:
  x-auth-signature: <base64url-encoded-signature>
  x-auth-timestamp: <unix-timestamp>
  x-auth-device-id: <device-id>
Body:
{
  "scope": "openid profile"
}
```

**Response**:
```json
{
  "access_token": "<jwt>",
  "token_type": "Bearer",
  "expires_in": 3600,
  "scope": "openid profile"
}
```

**Signature Verification**:
1. Fetch device's public key from database
2. Canonical payload: `timestamp + "\n" + method + "\n" + path + "\n" + body`
3. Verify ECDSA/RSA signature
4. Generate JWT signed with backend's RSA key

---

## BFF Revamp

### Changes

- Remove old session endpoints
- Use new `flow_*` tables via `backend-flow-sdk`

### New Endpoints

```yaml
paths:
  /bff/sessions:
    get:
      summary: List user's sessions
    post:
      summary: Create new session
      
  /bff/sessions/{sessionId}:
    get:
      summary: Get session with flows
      
  /bff/sessions/{sessionId}/flows:
    get:
      summary: List flows in session
    post:
      summary: Add flow to session
      
  /bff/flows/{flowId}:
    get:
      summary: Get flow with steps
    
  /bff/flows/{flowId}/steps:
    get:
      summary: List steps with status
    
  /bff/steps/{stepId}:
    get:
      summary: Get step details
    post:
      summary: Submit input for waiting step
```

---

## CLI Commands

### Structure

```rust
#[derive(Subcommand)]
enum Command {
    /// Start the server
    Server {
        #[arg(short, long)]
        import: Option<PathBuf>,  // -i flag for inline import
    },
    
    /// Start the worker
    Worker {
        #[arg(short, long)]
        import: Option<PathBuf>,
    },
    
    /// Start both server and worker
    Shared {
        #[arg(short, long)]
        import: Option<PathBuf>,
    },
    
    /// Export flow definitions
    Export {
        /// Export specific: session-type, flow-type, step-type, or step-id
        target: Option<String>,
        /// Export all
        #[arg(long)]
        all: bool,
        /// Output file (stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    
    /// Import flow definitions
    Import {
        /// Input file
        file: PathBuf,
        /// Validate only, don't persist
        #[arg(long)]
        dry_run: bool,
    },
}
```

### Usage Examples

```bash
# Start server with inline import
backend server -i flows/phone_otp.yaml

# Export all
backend export --all -o all_flows.yaml

# Export specific flow type
backend export PHONE_OTP -o phone_otp.yaml

# Export specific step by human ID
backend export "kyc.2026-03-15.phone_otp.send"

# Import
backend import flows/phone_otp.yaml

# Validate only
backend import flows/phone_otp.yaml --dry-run
```

---

## Declarative Flow Definition (YAML)

### Example

```yaml
# flows/phone_otp.yaml
api_version: flow/v1
kind: Flow
metadata:
  flow_type: PHONE_OTP
  human_id_prefix: phone_otp
  feature: flow-phone-otp
spec:
  steps:
    - step_type: SEND_OTP
      actor: SYSTEM
      human_id: send
      config:
        ttl_seconds: 300
        channel: sms
      on_success: VERIFY_OTP
      
    - step_type: VERIFY_OTP
      actor: END_USER
      human_id: verify
      config:
        max_attempts: 5
      on_success: COMPLETE
      on_failure: FAILED
      
    - step_type: COMPLETE
      actor: SYSTEM
      human_id: complete
```

### Import Validation

```rust
pub fn import_flow_definition(definition: &FlowDefinition) -> Result<(), FlowError> {
    for step in &definition.steps {
        if let Some(feature) = step.feature() {
            if !is_feature_enabled(feature) {
                return Err(FlowError::FeatureNotEnabled {
                    feature: feature.to_owned(),
                    step: step.step_type().to_owned(),
                });
            }
        }
    }
    // ...
}
```

---

## JWT & JWKS Implementation

### Key Management

```rust
pub struct SigningKeyManager {
    pool: Pool<AsyncPgConnection>,
}

impl SigningKeyManager {
    /// Get the current active signing key
    pub async fn get_active_key(&self) -> Result<ActiveKey, Error>;
    
    /// Generate new key, mark old as inactive
    pub async fn rotate_key(&self) -> Result<(), Error>;
    
    /// Get JWKS for public consumption
    pub async fn get_jwks(&self) -> Result<JwkSet, Error>;
}

pub struct TokenIssuer {
    keys: Arc<SigningKeyManager>,
}

impl TokenIssuer {
    pub async fn issue_token(
        &self,
        subject: &str,
        audience: &str,
        scopes: &[&str],
        ttl: Duration,
    ) -> Result<String, Error>;
}
```

---

## Cargo Features

### Workspace

```toml
# workspace Cargo.toml
[workspace.dependencies]
backend-flow-sdk = { path = "app/crates/backend-flow-sdk" }

# For webhook HTTP step
jsonpath_lib = "0.3"
```

### backend-server

```toml
# backend-server/Cargo.toml
[features]
default = ["flow-phone-otp"]

# Core SDK
flow-sdk = ["backend-flow-sdk"]

# Flow types (each gets its own feature)
flow-phone-otp = ["flow-sdk"]
flow-email-magic = ["flow-sdk"]
flow-first-deposit = ["flow-sdk"]
flow-id-document = ["flow-sdk"]
flow-address-proof = ["flow-sdk"]
flow-external-kyc = ["flow-sdk", "step-webhook-http"]

# Account management
flow-device-enroll = ["flow-sdk"]
flow-account-update = ["flow-sdk"]

# Admin flows (staff only)
flow-admin-user-management = ["flow-sdk"]

# Built-in step types
step-webhook-http = ["jsonpath_lib", "reqwest"]

# Auth features
auth-token-exchange = []
auth-jwks = []
```

---

## Workspace Structure

```
app/
├── bins/
│   └── backend/           # Main binary + CLI
├── crates/
│   ├── backend-auth/      # Add device signature verification, JWT issuer
│   ├── backend-core/      # Unchanged
│   ├── backend-flow-sdk/  # NEW - flow SDK
│   ├── backend-id/        # Add flow_* CUID prefixes
│   ├── backend-migrate/   # Add migration scripts
│   ├── backend-model/     # Add flow_* table models
│   ├── backend-repository/# Add flow repo traits
│   └── backend-server/    # Add /auth surface, revamp /bff
└── gen/
    ├── oas_server_auth/   # NEW - generated from openapi/user-auth.yaml
    ├── oas_server_bff/    # Updated
    ├── oas_server_kc/     # Unchanged
    └── oas_server_staff/  # Unchanged

openapi/
├── user-auth.yaml         # NEW
├── user-storage-bff.yaml  # Updated
├── user-storage--staff.yaml
└── user-storage-kc.yaml
```

---

## Implementation Phases

| Phase | Tasks | Priority |
|-------|-------|----------|
| **1. SDK Foundation** | Create `backend-flow-sdk` crate with core traits, ID types, registry | High |
| **2. Schema & Migration** | Add new tables, write migration sm_* → flow_*, add DB models | High |
| **3. Auth Surface** | Create `openapi/user-auth.yaml`, implement enrollment/device/token endpoints | High |
| **4. BFF Revamp** | Update OpenAPI, implement new session/flow/step endpoints | High |
| **5. CLI** | Add export/import commands, `-i` flag to startup | Medium |
| **6. Feature Flags** | Add cargo features, gate flow registration | Medium |
| **7. Tests** | Unit tests for SDK, integration tests for new surfaces | Medium |

---

## Step Definition Example

```rust
use backend_flow_sdk::{Actor, Step, StepContext, StepOutcome, HumanReadableId, FlowError};

pub struct SendOtpStep;

impl Step for SendOtpStep {
    fn step_type(&self) -> &'static str { "SEND_OTP" }
    fn actor(&self) -> Actor { Actor::System }
    fn human_id(&self) -> &'static str { "send" }
    fn feature(&self) -> Option<&'static str> { Some("flow-phone-otp") }
    
    async fn execute(&self, ctx: &StepContext) -> Result<StepOutcome, FlowError> {
        let msisdn = ctx.input["msisdn"].as_str()
            .ok_or_else(|| FlowError::MissingInput("msisdn"))?;
        
        let otp = generate_otp();
        ctx.sms_provider.send(msisdn, &otp).await?;
        
        // Store OTP hash in output for verification step
        let output = json!({
            "otp_hash": hash_otp(&otp),
            "expires_at": Utc::now() + Duration::minutes(5),
            "tries_left": 5
        });
        
        Ok(StepOutcome::Done)
    }
}

pub struct VerifyOtpStep;

impl Step for VerifyOtpStep {
    fn step_type(&self) -> &'static str { "VERIFY_OTP" }
    fn actor(&self) -> Actor { Actor::EndUser }
    fn human_id(&self) -> &'static str { "verify" }
    fn feature(&self) -> Option<&'static str> { Some("flow-phone-otp") }
    
    async fn validate_input(&self, input: &Value) -> Result<(), FlowError> {
        let code = input["code"].as_str().unwrap_or("");
        if code.len() != 6 || !code.chars().all(|c| c.is_numeric()) {
            return Err(FlowError::InvalidInput("OTP must be 6 digits"));
        }
        Ok(())
    }
    
    async fn execute(&self, ctx: &StepContext) -> Result<StepOutcome, FlowError> {
        // Called after validate_input succeeds
        let code = ctx.input["code"].as_str().unwrap();
        let stored = ctx.previous_step_output("SEND_OTP")?;
        
        if verify_otp(code, stored["otp_hash"].as_str().unwrap()) {
            Ok(StepOutcome::Done)
        } else {
            Ok(StepOutcome::Failed { 
                error: "Invalid OTP".into(), 
                retryable: true 
            })
        }
    }
}
```

---

## Built-in Step Types

### Webhook HTTP Step

A flexible, configurable step type for making HTTP calls with various behaviors. This step enables integration with external systems during flow execution.

#### Step Type

```
WEBHOOK_HTTP
```

#### Actor

`System` (executed by background worker)

#### Configuration Schema

```yaml
step_type: WEBHOOK_HTTP
actor: SYSTEM
human_id: call_external_api
config:
  # HTTP request configuration
  request:
    method: POST                    # POST, GET, PUT, DELETE, PATCH
    url: "https://api.external.com/endpoint"
    headers:
      Authorization: "Bearer {{context.api_token}}"
      Content-Type: "application/json"
    timeout_seconds: 30
    retry:
      max_attempts: 3
      backoff_seconds: 5
    
  # Payload template (supports variable interpolation)
  payload:
    user_id: "{{session.user_id}}"
    phone: "{{flow.context.phone}}"
    action: "verify_identity"
    
  # Execution behavior
  behavior: wait_and_save           # fire_and_forget | wait_for_response | wait_and_save
  
  # Response extraction (only for wait_and_save behavior)
  # Uses JSONPath-like query syntax
  extract:
    to_save:
      external_id:
        query: "$.user.id"
      external_door_id:
        query: "$.door.id"
      verification_status:
        query: "$.status"
      raw_response:
        query: "$"                  # Entire response body
        
  # Optional: Save extracted data to specific locations
  save_to:
    flow_context: true              # Save to flow.context
    session_context: false          # Save to session.context
    user_metadata: true            # Save to user's metadata (for JWT claims)
    user_metadata_prefix: "external_" # Prefix for user metadata keys
    
  # Success conditions
  success_when:
    status_code: 200                # HTTP status code
    # OR use response body condition
    # response:
    #   path: "$.status"
    #   equals: "success"
```

#### Behavior Types

| Behavior | Description | Output |
|----------|-------------|--------|
| `fire_and_forget` | Send request, don't wait for response | `{ sent: true }` |
| `wait_for_response` | Wait for response, store raw response | `{ status: 200, body: {...} }` |
| `wait_and_save` | Wait, extract fields, save to configured locations | Extracted data per `extract` config |

#### Variable Interpolation

The payload supports variable interpolation using `{{variable.path}}` syntax:

| Variable Source | Example | Description |
|-----------------|---------|-------------|
| `{{session.*}}` | `{{session.user_id}}` | Session fields |
| `{{flow.context.*}}` | `{{flow.context.phone}}` | Flow context data |
| `{{step.PREVIOUS_STEP.output.*}}` | `{{step.send_otp.output.otp_ref}}` | Previous step output |
| `{{env.VAR_NAME}}` | `{{env.API_KEY}}` | Environment variables |
| `{{config.*}}` | `{{config.external_service.url}}` | Backend config |

#### Query Language

The `extract.to_save.*.query` field uses a JSONPath-like syntax:

```
$                    # Root object
$.user.id            # Nested field access
$.users[0].id        # Array index access
$.users[*].id        # All array elements (returns array)
$.data.items[0:3]    # Array slice
$.store..price       # Recursive descent (all price fields)
$.items[*].name      # All names in items array
```

#### YAML Example: External KYC Provider Integration

```yaml
# flows/external_kyc.yaml
api_version: flow/v1
kind: Flow
metadata:
  flow_type: EXTERNAL_KYC
  human_id_prefix: external_kyc
  feature: flow-external-kyc
spec:
  steps:
    - step_type: WEBHOOK_HTTP
      actor: SYSTEM
      human_id: call_kyc_provider
      config:
        request:
          method: POST
          url: "https://kyc-provider.example.com/api/v1/verify"
          headers:
            Authorization: "Bearer {{env.KYC_PROVIDER_TOKEN}}"
            Content-Type: "application/json"
          timeout_seconds: 45
          retry:
            max_attempts: 3
            backoff_seconds: 10
        payload:
          reference_id: "{{session.id}}"
          user_phone: "{{flow.context.phone}}"
          callback_url: "https://our-backend.example.com/kyc/callback"
        behavior: wait_and_save
        extract:
          to_save:
            external_verification_id:
              query: "$.verification_id"
            provider_status:
              query: "$.status"
            estimated_completion:
              query: "$.eta_seconds"
        save_to:
          flow_context: true
          user_metadata: true
          user_metadata_prefix: "kyc_"
        success_when:
          status_code: 200
      on_success: WAIT_CALLBACK
      
    - step_type: WAIT_EXTERNAL
      actor: SYSTEM
      human_id: wait_callback
      config:
        wait_for_event: "KYC_CALLBACK"
        timeout_seconds: 86400    # 24 hours
      on_success: FETCH_RESULT
      
    - step_type: WEBHOOK_HTTP
      actor: SYSTEM
      human_id: fetch_result
      config:
        request:
          method: GET
          url: "https://kyc-provider.example.com/api/v1/result/{{flow.context.external_verification_id}}"
          headers:
            Authorization: "Bearer {{env.KYC_PROVIDER_TOKEN}}"
        behavior: wait_and_save
        extract:
          to_save:
            kyc_status:
              query: "$.result.status"
            kyc_score:
              query: "$.result.risk_score"
            verification_documents:
              query: "$.result.documents"
        save_to:
          flow_context: true
          user_metadata: true
      on_success: COMPLETE
```

#### Rust Implementation

```rust
// In backend-server/src/flow/steps/webhook_http.rs

use backend_flow_sdk::{Actor, Step, StepContext, StepOutcome, FlowError};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookHttpConfig {
    pub request: HttpRequestConfig,
    pub payload: Value,
    pub behavior: WebhookBehavior,
    pub extract: Option<ExtractConfig>,
    pub save_to: Option<SaveToConfig>,
    pub success_when: Option<SuccessCondition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpRequestConfig {
    pub method: String,
    pub url: String,
    pub headers: HashMap<String, String>,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    pub retry: Option<RetryConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WebhookBehavior {
    FireAndForget,
    WaitForResponse,
    WaitAndSave,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractConfig {
    pub to_save: HashMap<String, FieldExtract>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldExtract {
    pub query: String,  // JSONPath query
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveToConfig {
    #[serde(default = "default_true")]
    pub flow_context: bool,
    #[serde(default)]
    pub session_context: bool,
    #[serde(default)]
    pub user_metadata: bool,
    #[serde(default)]
    pub user_metadata_prefix: Option<String>,
}

pub struct WebhookHttpStep {
    config: WebhookHttpConfig,
    http_client: reqwest::Client,
}

impl Step for WebhookHttpStep {
    fn step_type(&self) -> &'static str { "WEBHOOK_HTTP" }
    fn actor(&self) -> Actor { Actor::System }
    fn human_id(&self) -> &'static str { "webhook_http" }
    fn feature(&self) -> Option<&'static str> { Some("step-webhook-http") }
    
    async fn execute(&self, ctx: &StepContext) -> Result<StepOutcome, FlowError> {
        // 1. Interpolate variables in URL, headers, and payload
        let interpolated = self.interpolate_config(ctx)?;
        
        // 2. Build HTTP request
        let request = self.build_request(&interpolated)?;
        
        // 3. Execute based on behavior
        match &self.config.behavior {
            WebhookBehavior::FireAndForget => {
                // Spawn and forget
                tokio::spawn(async move {
                    let _ = request.send().await;
                });
                Ok(StepOutcome::Done)
            }
            
            WebhookBehavior::WaitForResponse => {
                let response = self.execute_with_retry(request).await?;
                let output = json!({
                    "status": response.status().as_u16(),
                    "body": response.json::<Value>().await.ok()
                });
                Ok(StepOutcome::Done)
            }
            
            WebhookBehavior::WaitAndSave => {
                let response = self.execute_with_retry(request).await?;
                
                // Check success condition
                if !self.check_success_condition(&response, &self.config.success_when) {
                    return Ok(StepOutcome::Failed {
                        error: "HTTP response did not match success condition".into(),
                        retryable: true,
                    });
                }
                
                // Extract fields using JSONPath
                let body: Value = response.json().await?;
                let extracted = self.extract_fields(&body, &self.config.extract)?;
                
                // Save to configured locations
                self.save_extracted_data(ctx, &extracted, &self.config.save_to).await?;
                
                Ok(StepOutcome::Done)
            }
        }
    }
}

impl WebhookHttpStep {
    fn interpolate_config(&self, ctx: &StepContext) -> Result<WebhookHttpConfig, FlowError> {
        // Interpolate {{variable}} placeholders in URL, headers, payload
        let interpolator = VariableInterpolator::new(ctx);
        let mut config = self.config.clone();
        
        config.request.url = interpolator.interpolate(&config.request.url)?;
        for (key, value) in &mut config.request.headers {
            *value = interpolator.interpolate(value)?;
        }
        config.payload = interpolator.interpolate_value(&config.payload)?;
        
        Ok(config)
    }
    
    fn extract_fields(
        &self,
        body: &Value,
        extract_config: &Option<ExtractConfig>,
    ) -> Result<Value, FlowError> {
        let Some(config) = extract_config else {
            return Ok(json!({}));
        };
        
        let mut extracted = serde_json::Map::new();
        for (name, field_config) in &config.to_save {
            let value = jsonpath_lib::select(body, &field_config.query)
                .map_err(|e| FlowError::QueryError(e.to_string()))?
                .first()
                .cloned()
                .unwrap_or(Value::Null);
            extracted.insert(name.clone(), value);
        }
        
        Ok(Value::Object(extracted))
    }
    
    async fn save_extracted_data(
        &self,
        ctx: &StepContext,
        extracted: &Value,
        save_config: &Option<SaveToConfig>,
    ) -> Result<(), FlowError> {
        let Some(config) = save_config else {
            return Ok(());
        };
        
        if config.flow_context {
            ctx.update_flow_context(extracted).await?;
        }
        
        if config.session_context {
            ctx.update_session_context(extracted).await?;
        }
        
        if config.user_metadata {
            let prefix = config.user_metadata_prefix.as_deref().unwrap_or("");
            ctx.update_user_metadata(extracted, prefix).await?;
        }
        
        Ok(())
    }
}
```

#### Using Extracted Data in JWT

The extracted data saved to `user_metadata` can be included in JWT claims:

```rust
// In token issuer
impl TokenIssuer {
    pub async fn issue_token(
        &self,
        user_id: &str,
        device_id: &str,
        scopes: &[&str],
        ttl: Duration,
    ) -> Result<String, Error> {
        // Fetch user metadata (populated by webhook steps)
        let user = self.user_repo.get_user(user_id).await?;
        let metadata = user.metadata.unwrap_or_default();
        
        let claims = Claims {
            sub: user_id.to_owned(),
            device_id: device_id.to_owned(),
            scopes: scopes.to_vec(),
            // Include extracted data from KYC flows
            external_id: metadata.get("kyc_external_id").cloned(),
            verification_status: metadata.get("kyc_provider_status").cloned(),
            // ... other claims
            exp: Utc::now().timestamp() + ttl.as_secs() as i64,
            iat: Utc::now().timestamp(),
        };
        
        self.sign_claims(&claims).await
    }
}
```

#### Returning Extracted Data from User Endpoints

The metadata populated by webhook steps can be returned via the user info endpoint:

```yaml
# openapi/user-auth.yaml
paths:
  /auth/userinfo:
    get:
      summary: Get current user info with KYC metadata
      security:
        - BearerAuth: []
      responses:
        200:
          content:
            application/json:
              schema:
                type: object
                properties:
                  user_id:
                    type: string
                  phone:
                    type: string
                  email:
                    type: string
                  kyc_status:
                    type: string
                  # Dynamic fields from webhook extractions
                  metadata:
                    type: object
                    additionalProperties: true
                    example:
                      kyc_external_id: "ext-12345"
                      kyc_provider_status: "verified"
                      kyc_score: 95
```

```rust
// In backend-server/src/api/auth/user.rs
pub async fn get_user_info(
    claims: &JwtToken,
    state: Arc<AppState>,
) -> Result<UserInfo, Error> {
    let user = state.user_repo.get_user(&claims.sub).await?
        .ok_or_else(|| Error::not_found("USER_NOT_FOUND", "User not found"))?;
    
    // Get sessions with their extracted data
    let sessions = state.flow_repo.list_sessions(&claims.sub).await?;
    
    Ok(UserInfo {
        user_id: user.user_id.clone(),
        phone: user.phone_number,
        email: user.email,
        kyc_status: derive_kyc_status(&sessions),
        // Metadata from webhook extractions
        metadata: user.metadata.unwrap_or_default(),
    })
}
```

#### Feature Flag

```toml
# backend-server/Cargo.toml
[features]
# Webhook step type
step-webhook-http = ["jsonpath_lib", "reqwest"]
```

---

## Open Questions

1. **Key Rotation Policy**: What's the default rotation period for RSA signing keys?
2. **Token Scopes**: Define standard scope values for different token use cases
3. **Approval Flow**: Exact flow for multi-approver scenarios (if needed)
4. **Session Types**: Finalize list of session types beyond KYC_FULL
5. **Webhook Security**: Should webhook endpoints support mTLS or API key rotation?
6. **Query Language**: Should we use JSONPath, JMESPath, or a custom DSL for extraction?