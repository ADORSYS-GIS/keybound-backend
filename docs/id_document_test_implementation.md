# Technical Implementation Plan: ID Document KYC Flow Tests

## Overview

This document provides a technical implementation plan specifically for Rust/Axum/Diesel-async backend in this codebase. It translates the conceptual test plan in `id_document_test_plan.md` to concrete implementation details including crates structure, mocking strategies, and testing frameworks.

## Implementation Status

| Test Layer                              | Status        |
| --------------------------------------- | ------------- |
| BDD End-to-End Flow Tests (Cucumber)    | **COMPLETED** |
| Integration + Service Tests (Rust Test) | **PENDING**   |
| State-Machine-Specific Unit Tests       | **PENDING**   |

### BDD End-to-End Flow Tests - Implementation Details

The Cucumber feature file has been fully implemented for the ID Document KYC flow.

**Feature File Location:**

- `app/crates/backend-e2e/tests/features/full/id_document.feature`

**Step Definitions:**

- Located in `app/crates/backend-e2e/tests/cucumber_full.rs`

**Scenarios Implemented:**

1. **Flow Initiation** - ID Document KYC Flow (line 10-23)
2. **Document Upload** - Valid ID Document Upload (line 26-38)
3. **Manual Review** - Staff Approval of ID Document (line 41-46)
4. **Manual Review Rejection** - Staff Rejection of ID Document (line 49-54)
5. **Flow Completion** - User Profile Update After Approval (line 57-65)

**Verification Status:**

- `cargo check --workspace` passes successfully
- All scenario definitions include proper assertions for state machine persistence (`sm_instance`, `sm_event`)

## Implementation Scope

### Flow Under Test

- **Flow Definition**: `flows/id_document.yaml` - orchestrates ID document KYC verification
- **Implementation**: `app/crates/backend-server/src/flows/definitions/id_document.rs`
- **Storage**: Document uploads handled via `MinioStorage` trait
- **State Machine**: Uses `backend-flow-sdk` session management + persistence

### Test Layers

1. **BDD End-to-End Flow Tests (Cucumber)**
   - Location: `app/crates/backend-e2e/tests/features/full/id_document.feature`
   - Test Env: Full stack with Docker containers (Postgres, MinIO, Redis)
   - Traffic: Real HTTP via BFF APIs toward actual Flow SDK runtime

2. **Integration + Service Tests (Rust Test)**
   - Location: `app/crates/backend-server/tests/id_document_flow.rs`
   - Test Env: In-process Axum router "tower::Service" (no TCP)
   - Mocking: `MockMinioStorage`, `MockStateMachineRepo`, `MockOidcState`

3. **State-Machine-Specific Unit Tests**
   - Location: Inline unit tests in `app/crates/backend-server/src/flows/definitions/id_document.rs`
   - Test Env: Pure unit tests; mock individual repositories (DB, queue, storage)

## 1. Core State Transitions – Implementation

### Scenario: Flow Initiation

**Concrete Steps**

1. Builder injects a fresh Postgres UUID and JWT (BFF device) into `World`
2. Cucumber: `When("user calls POST /api/kyc/id\u2011document")`
3. Test invokes BFF controller `start_id_document` handler
4. Requires mocks:
   - `MockStateMachineRepo` → insert new session record returning session ID / `AwaitingDocumentUpload` state
   - NO null `user_id` (BFF already has device registration)

**Assertion**

- HTTP 200, JSON body contains session ID
- `sm_instance` row persisted for `KYC_ID_DOCUMENT` definition with `import_id = 1`

### Scenario: Document Upload Success

**Setup (test data injection)**

- `World` contains prior session ID (step above)
- File upload simulation (inline string, Base64 binary, store on disk)

**Concrete Steps**

- Cucumber: `When("user uploads valid id_verification documents for session {string}")`
- Payload:

```json
{
  "front": { "filename": "dl_front.png", "base64": "…", "mime": "image/png" },
  "back": { "filename": "dl_back.png", "base64": "…", "mime": "image/png" }
}
```

**Mocks**

- `MockMinioStorage::put_object` returns OK (saves `url-dl` as `s/i/xx/ii.png`)
- `MockStateMachineRepo::transition` records attempted transition with timestamp

**Assertions**

- HTTP 200
- `sm_event` entries exist for `DocumentUploaded` event
- Session still exists in `sm_instance`

### Scenario: Manual Review (Staff Approval API)

**Preconditions**

- StateMachine contains session with step that contains flag `AWAITING_REVIEW`
- `Cucumber`: `Given("an id_document session OBJECTIVITY\u2011PENDING")`

**API Call**
`
