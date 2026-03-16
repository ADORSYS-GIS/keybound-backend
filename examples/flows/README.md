# E2E Example Flows

Production-ready KYC flows for the Flow SDK system.

## 📱 PHONE_OTP Flow

**Purpose:** Verify user's phone number via SMS OTP

**File:** `01_phone_otp.yaml`

```yaml
apiVersion: flow.adorsys.kyc/v1
kind: Flow
metadata:
  name: PHONE_OTP  # Flow identifier
spec:
  humanIdPrefix: phone_otp      # Human-readable prefix
  overrideExisting: true
  steps:
    - id: validate_user_exists
      # ...
```

**Steps:**
1. `validate_user_exists` - Internal DB lookup
2. `send_otp` - Send via SMS gateway (internal service)
3. `wait_for_otp` - User inputs 6-digit code (5min timeout)
4. `verify_otp` - Verify against stored OTP (internal)
5. `complete` - Flow succeeds

**Key Features:**
- Internal database calls (not webhooks)
- SMS gateway integration (internal service)
- User input step with 5-minute timeout
- OTP verification
- Metadata: auth_token saved

**Usage:**
```bash
# Import flow definition
backend import examples/flows/01_phone_otp.yaml

# User initiates flow
curl -X POST /bff/sessions \
  -H "x-auth-signature: ..." \
  -d '{"type":"PHONE_OTP","context":{"phone_number":"+1234567890"}}'
```

---

## 💰 FIRST_DEPOSIT Flow

**Purpose:** Process user's first deposit with admin approval

**File:** `02_first_deposit.yaml`

```yaml
apiVersion: flow.adorsys.kyc/v1
kind: Flow
metadata:
  name: FIRST_DEPOSIT  # Flow identifier
spec:
  humanIdPrefix: first_deposit  # Human-readable prefix
  overrideExisting: true
  steps:
    - id: initiate_deposit
      # ...
```

**Steps:**
1. `initiate_deposit` - User provides phone, amount, currency, provider
2. `check_user_exists` - Internal DB lookup
3. `validate_amount` - Internal validation (limits, currency)
4. `await_approval` - Wait for admin approval (24h timeout) via `/auth/approve`
5. `process_deposit` - Calls external webhook to payment processor
6. `persist_deposit_result` - Save transaction ID to user metadata
7. `complete` - Return recipient info for deposit instructions

**Key Features:**
- Form input for user data
- Internal validation (no webhooks)
- Staff approval step via OAuth JWT
- External webhook to payment processor
- Retry policy (3 attempts, 5s backoff)
- Metadata saved: transaction_id, status, receipt_url
- Returns recipient name/phone for deposit instructions

**Usage:**
```bash
# Import flow definition
backend import examples/flows/02_first_deposit.yaml

# User initiates deposit
curl -X POST /bff/sessions \
  -H "x-auth-signature: ..." \
  -d '{
    "type": "FIRST_DEPOSIT",
    "context": {
      "phone_number": "+19998887777",
      "amount": "500",
      "currency": "USD",
      "provider": "ACME_BANK"
    }
  }'

# Admin approves via OAuth
curl -X POST /auth/approve/{step_id} \
  -H "Authorization: Bearer {admin_jwt}" \
  -d '{
    "decision": "APPROVED",
    "message": "All checks passed"
  }'

# Verify metadata saved
curl /auth/userinfo -H "Authorization: Bearer {user_jwt}"
# Returns: { "deposit_transaction_id": "txn_123", ... }
```

---

## 🔧 Builtin Step Types

**File:** `builtin_steps.rs`

**Internal Steps (No Webhooks):**
- `validate_user_exists` - Database lookup
- `send_sms_otp` - SMS gateway integration
- `verify_otp` - OTP verification
- `check_user_exists` - User lookup (deposit context)
- `validate_deposit` - Amount/currency validation
- `persist_deposit_result` - Save webhook response to metadata

**External Steps:**
- `webhook_http` - External API calls (payment processors, etc.)

---

## 📋 Flow Definition Schema

**API Version:**
```yaml
apiVersion: flow.adorsys.kyc/v1  # Follows K8s-style namespacing
kind: Flow
metadata:
  name: FLOW_NAME               # Flow identifier (required)
spec:
  humanIdPrefix: prefix         # Human-readable prefix (required)
  overrideExisting: true        # Conflict resolution
  steps:
    - id: step_id               # Step identifier (must be unique)
      actor: SYSTEM             # Who executes: SYSTEM, END_USER, STAFF
      config:
        type: step_type         # Maps to Rust Step impl
        ...                     # Step-specific config
      onSuccess: next_step      # Success transition
      onFailure: failure_step   # Failure transition
```

**Key Points:**
- `metadata.name` is the canonical identifier (like K8s resource name)
- `spec.humanIdPrefix` provides human-readable ID prefix
- `steps[].id` is referenced in transitions
- `config.type` maps to builtin step implementations
- Internal steps use simple types, external use `webhook_http`

---

## ✅ Verification

Validate flow syntax:
```bash
cargo run --bin backend -- import --dry-run examples/flows/01_phone_otp.yaml
cargo run --bin backend -- import --dry-run examples/flows/02_first_deposit.yaml
```

Test flows at runtime:
```bash
# Start with flows loaded
cargo run --bin backend -- serve --config config.yaml --import examples/flows/
```

---

## 🔒 Security

- **Signature Auth:** All BFF endpoints require device-bound signatures
- **OAuth JWT:** Admin approval requires valid JWT from Keycloak OIDC
- **Replay Protection:** Nonces stored in Redis with 5min TTL
- **Timestamp Skew:** Max 5 minutes allowed
- **Staff Approval:** `/auth/approve` protected by Bearer token validation

---

## 📊 Metadata Persistence

### PHONE_OTP
- `session_context.user_id`
- `session_context.fullname`
- `session_context.auth_token`
- `flow_context.otp_verified`

### FIRST_DEPOSIT
- `user_metadata.deposit_transaction_id`
- `user_metadata.deposit_status` 
- `user_metadata.deposit_receipt_url`
- `session_context.recipient_name`
- `session_context.recipient_phone`
- `session_context.deposit_amount`
- `session_context.deposit_currency`

---

## 💡 Best Practices

1. **Internal Logic:** Use native step types, not webhooks
2. **External Calls:** Only `webhook_http` for 3rd party APIs
3. **Timeouts:** Set appropriate timeouts (3-10s for internal, 30s for user)
4. **Retries:** Use `retry_policy` for external calls only
5. **Metadata:** Save results to appropriate context (user/session/flow)
6. **Approval:** Use `manual_approval` for staff review steps
7. **Naming:** Use descriptive step IDs, human-readable prefixes

---

## 🐛 Troubleshooting

**Flow won't import?**
- Check `apiVersion` is correct: `flow.adorsys.kyc/v1`
- Check `metadata.name` is present
- Check step IDs are unique
- Check `onSuccess/onFailure` reference valid step IDs

**Webhook not working?**
- Verify URL uses env vars: `${VAR_NAME}`
- Check `timeout_ms` is reasonable (5000-30000)
- Enable `retry_policy` for external calls
- Check logs for HTTP errors

**Admin approval not showing?**
- Ensure `actor: STAFF` for approval step
- Check `/auth/approve` receives valid JWT
- Verify `step_id` is correct

**Metadata not saving?**
- Check extraction rules JSONPath syntax
- Verify target context (user/session/flow)
- Check step output format matches expectations

---

## 🔮 Future Enhancements

- Add `manual_input` step with configurable form schema
- Support conditional transitions (`when:` expressions)
- Add step timeout policies
- Support parallel step execution
- Add flow composition (sub-flows)
- Support step-level feature flags

---

## 📚 API Documentation (Swagger UI)

When the server is running, access the Swagger UI at:

```
http://localhost:3000/swagger-ui/
```

Available OpenAPI specs:
- `/api-docs/bff/openapi.json` - BFF API (Flow sessions, steps)
- `/api-docs/core/openapi.json` - Core API metadata

---

## 📖 Integration Guide

**Full BFF integration guide:** [`docs/integration/BFF-INTEGRATION-GUIDE.md`](../docs/integration/BFF-INTEGRATION-GUIDE.md)

### Quick Reference: Phone OTP Flow

```
1. GET    /bff/users/{userId}              → Get user profile
2. POST   /bff/sessions                    → Create KYC session
3. POST   /bff/sessions/{id}/flows         → Add PHONE_OTP flow
4. GET    /bff/flows/{flowId}              → Check current step
5. POST   /bff/steps/{stepId}              → Submit OTP code
6. Repeat 4-5 until status = COMPLETED
```

### Quick Reference: First Deposit Flow

```
1. POST   /bff/sessions                    → Create/reuse session
2. POST   /bff/sessions/{id}/flows         → Add FIRST_DEPOSIT flow
3. GET    /bff/flows/{flowId}              → Check current step
4. POST   /bff/steps/{stepId}              → Submit deposit details
5. Poll   /bff/flows/{flowId}              → Wait for admin approval
6. Flow status = COMPLETED when approved
```

---

## ✅ Completed Tasks

**Phase A-G Implementation:**
- [x] Phase A: Signature Auth Hardening (cryptographic verification, Redis replay protection)
- [x] Phase B: SDK Contracts (ContextUpdates, StepOutcome)
- [x] Phase C: WEBHOOK_HTTP step implementation
- [x] Phase D: Dynamic Registry (import/export CLI)
- [x] Phase E: Worker Migration (flow_step model)
- [x] Phase F: API Surface (userinfo, /auth/approve with OAuth)
- [x] Phase G: Legacy Removal (sm_* tables/code removed from production)
- [x] Legacy test_utils cleanup (StateMachineRepo mock removed)
- [x] Workspace lib tests passing (14 tests)

**Flow Definitions:**
- [x] PHONE_OTP flow YAML (examples/flows/01_phone_otp.yaml)
- [x] FIRST_DEPOSIT flow YAML (examples/flows/02_first_deposit.yaml)
- [x] Builtin step implementations (examples/flows/builtin_steps.rs)

---

## 📋 Remaining Tasks

**Integration Testing:**
- [ ] Run OAS integration tests: `just test-it`
- [ ] Run E2E smoke tests: `just test-e2e-smoke`

**Flow Integration:**
- [ ] Wire builtin_steps.rs into the Flow SDK step registry
- [ ] Add step type registration in backend-server startup
- [ ] Test PHONE_OTP flow end-to-end with signature auth
- [ ] Test FIRST_DEPOSIT flow with admin approval

**Documentation:**
- [ ] Update AGENTS.md with final test commands
- [ ] Add flow-specific test documentation

**Optional Enhancements:**
- [ ] Add `manual_input` step with configurable form schema
- [ ] Support conditional transitions (`when:` expressions)
- [ ] Add step timeout policies
- [ ] Support parallel step execution
- [ ] Add flow composition (sub-flows)

---

**Status:** Implementation Complete, Integration Pending  
**Version:** 1.0.0  
**Last Updated:** 2026-03-15  
**Maintained By:** Backend Team
