# Azamra Backend Stabilization & Fixes Report

This document details the major technical fixes implemented to resolve blocking errors in the local development environment for the Azamra KYC and Deposit flows.

## 1. Deposit 403 Forbidden (Fineract Account Required)

### Error Symptoms
When attempting a deposit after successful KYC, the request failed with:
`{"errorKey":"auth.forbidden","status":403,"message":"Fineract account required. Complete KYC to enable payments."}`

### Root Cause Analysis
The `azamra-bff` (proxy) fetches the user's KYC status from `user-storage`. However, Keycloak injected a federation prefix `f:backend-user-storage:` into the user ID. The backend router extracted a clean ID from the token but the path `{user_id}` still contained the prefix, leading to an authorization mismatch (`caller_id != user_id`). 

Additionally, the KYC flow was saving the Fineract Client ID under the wrong metadata key.

### Implemented Fixes

#### A. Keycloak Prefix Stripping
Modified `app/crates/backend-server/src/api/bff_flow/service.rs` to handle both pure and federated IDs:

```rust
pub async fn get_user(api: &BackendApi, user_id: String, caller_id: String) -> Result<UserResponse, Error> {
    // Strips 'f:backend-user-storage:' prefix if present
    let clean_user_id = user_id.rsplit(':').next().unwrap_or(&user_id).to_string();
    
    if clean_user_id != caller_id {
        return Err(Error::unauthorized("Cannot access other users' data"));
    }
    // ...
}
```

#### B. Metadata Key Naming Alignment
Fixed `flows/first_deposit.yaml` to use the correct key name expected by the `azamra-tokenization-bff` service.

- **The Problem**: The BFF's `UserContextService` specifically looks for a field named `fineractClientId` in the user's metadata profile to authorize payments. If this field is missing, the BFF logs a warning (`fineractClientId not found in user storage metadata`) and returns a **403 Forbidden** error.
- **Detailed Rationale**: The initial KYC flow in `first_deposit.yaml` was reading the Fineract ID from the CUSS response correctly (using the path `/step_output/cuss_register_customer/fineractClientId`), but it was **saving** it into a metadata key called `fineractId`. In the `user-storage` database (`app_user_data` table), metadata is stored as individual rows. Since the keys didn't match (`fineractId` vs `fineractClientId`), the BFF's query to `user-storage` for the user's profile metadata didn't return the ID under the name it was programmed to look for.
- **The Fix**: Updated the `UPDATE_USER_METADATA` step in the YAML flow to target the correct key:

```yaml
  update_deposit_metadata:
    action: UPDATE_USER_METADATA
    config:
      mappings:
        - target_path: /fineractClientId  # Changed from /fineractId to match BFF expectations
          source: flow
          source_path: /step_output/cuss_register_customer/fineractClientId
          eager: true
```

---

## 2. Deposit 500 DNS Resolution Error

### Error Symptoms
`{"errorKey":"app.internal_error","status":500,"message":"Failed to resolve 'payment-gateway-service'"}`

### Root Cause Analysis
The `azamra-bff` and `payment-gateway-service` were on isolated Docker bridge networks despite having the same name in different compose files.

### Implemented Fixes
Update the network declaration in `docker-compose.yml` to use the external shared network:

```yaml
networks:
  fineract-dev-network:
    external: true
    name: fineract-dev-network
```

---

## 3. Portfolio 401 Unauthorized

### Error Symptoms
`{"errorKey":"auth.unauthorized","status":401,"message":"401 Unauthorized from GET http://customer-self-service:8080/api/accounts/savings"}`

### Root Cause Analysis
The BFF was attempting to call the live `customer-self-service` container, which rejects local development tokens. Local stubs were intended to be used.

### Implemented Fixes
Pointed the CUSS base URL to the wiremock service in `docker-compose.yml`:

```yaml
      APP_CUSS_BASE_URL: http://cuss-wiremock:8080
```

---

## 4. Asset Details 500 JSON Decoding Error

### Error Symptoms
`JSON decoding error: Cannot deserialize value of type ...PriceMode from String "AUTO": not one of the values accepted for Enum class: [MANUAL]`

### Root Cause Analysis
The wiremock mapping returned `"priceMode": "AUTO"`, but the BFF model only accepted `"MANUAL"`.

### Implemented Fixes
Updated `config/e2eTest/resources/wiremock/mappings/29-assets-detail.json`:

```json
- "priceMode": "AUTO",
+ "priceMode": "MANUAL",
```

---

## 5. General Infrastructure Stability

### SMS Gateway Health Failure
The `sms-gateway` was repeatedly failing because of image mismatch or missing entrypoint files.
**Fix**: Updated `deploy/compose/app.compose.yml` to include `restart: unless-stopped` and ensured consistent imagery.

### Keycloak Restart Loop
Keycloak was occasionally failing to start correctly due to resource races.
**Fix**: Added `restart: unless-stopped` to `deploy/compose/keycloak.compose.yml`.

---

### Summary of Staged Files
All above fixes have been staged for commit, including:
- `docker-compose.yml` (Consolidated environment and networks)
- `backend-server/src` (Identity and route fixes)
- `flows/first_deposit.yaml` (Metadata naming fix)
- `wiremock/mappings/*.json` (Mock stubs and Enum fixes)
