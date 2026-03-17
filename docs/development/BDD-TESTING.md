# BDD Testing Documentation

## Overview

### Why We Use Gherkin Feature Files

This repository uses a **hybrid BDD (Behavior-Driven Development)** approach where:

1. **Gherkin `.feature` files** serve as living documentation and behavioral specifications
2. **Rust tests** implement the scenarios directly using `wiremock` for HTTP mocking

This differs from traditional Cucumber frameworks (like `cucumber-rs`) by keeping the feature files as pure documentation/specification, while the test implementation remains idiomatic Rust.

### Benefits of This Approach

| Benefit | Description |
|---------|-------------|
| **Living Documentation** | `.feature` files describe expected behavior in plain language |
| **Stakeholder Communication** | Non-technical team members can read and validate scenarios |
| **Test Traceability** | Each Rust test maps to documented behavior |
| **No Framework Lock-in** | Pure Rust tests without Cucumber framework constraints |
| **Async Native** | Full `tokio` async support without cucumber-rs limitations |
| **IDE Support** | Standard Rust tooling works (debugger, test runner, coverage) |

### How It Fits Into Our Testing Strategy

```
┌─────────────────────────────────────────────────────────────────────┐
│                        Testing Pyramid                               │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│    ┌──────────────────┐                                             │
│    │   E2E Full       │  ← backend-e2e/tests/full.rs                │
│    │   (Compose)      │    Docker Compose, external deps             │
│    └────────┬─────────┘                                             │
│             │                                                        │
│    ┌────────▼─────────┐                                             │
│    │   E2E Smoke      │  ← backend-e2e/tests/smoke.rs               │
│    │   (Compose)      │    Quick health checks                       │
│    └────────┬─────────┘                                             │
│             │                                                        │
│    ┌────────▼─────────┐                                             │
│    │   Flow E2E       │  ← backend-server/tests/flow_cuss_e2e.rs    │
│    │   (Wiremock)     │    .feature files as spec                    │
│    └────────┬─────────┘                                             │
│             │                                                        │
│    ┌────────▼─────────┐                                             │
│    │   Integration    │  ← backend-server/src/api/it_tests.rs       │
│    │   (OAS Tests)    │    OpenAPI spec validation                   │
│    └────────┬─────────┘                                             │
│             │                                                        │
│    ┌────────▼─────────┐                                             │
│    │   Unit Tests     │  ← Inline #[test] in modules                │
│    │   (Mockall)      │    Fast, isolated, no external deps          │
│    └──────────────────┘                                             │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

## Setup Guide

### Required Dependencies

The testing setup uses these workspace dependencies (defined in root `Cargo.toml`):

```toml
# HTTP Mocking
wiremock = "0.6.5"

# Async Runtime
tokio = { version = "1", features = ["macros", "rt-multi-thread", "time"] }

# Serialization
serde = "1.0"
serde_json = "1.0"

# Testing Utilities
mockall = "0.14"
serial_test = "3.4.0"

# Database (for E2E)
tokio-postgres = "0.7.16"

# Cryptography (for BFF signature tests)
p256 = { version = "0.13", features = ["ecdsa"] }
argon2 = "0.5"
sha2 = "0.10"
hmac = "0.12"
base64 = "0.22"
```

### Directory Structure

```
app/crates/
├── backend-server/
│   └── tests/
│       ├── features/              # Gherkin feature files
│       │   └── cuss_deposit.feature
│       └── flow_cuss_e2e.rs       # Rust test implementation
│
├── backend-e2e/                   # Full stack E2E tests
│   └── tests/
│       ├── common/
│       │   └── mod.rs             # Shared test utilities
│       ├── smoke.rs                # Quick health checks
│       └── full.rs                 # Comprehensive scenario tests
│
└── backend-auth/
    └── tests/
        └── oidc_wiremock_e2e.rs    # Wiremock-based OIDC tests
```

### Configuration Files

**Feature-gated tests** in `Cargo.toml`:

```toml
# backend-server/Cargo.toml
[features]
e2e-tests = ["dep:wiremock"]

[[test]]
name = "flow_cuss_e2e"
path = "tests/flow_cuss_e2e.rs"
required-features = ["e2e-tests", "flow-sdk"]

# backend-e2e/Cargo.toml
[features]
e2e-tests = []

[[test]]
name = "smoke"
path = "tests/smoke.rs"
required-features = ["e2e-tests"]

[[test]]
name = "full"
path = "tests/full.rs"
required-features = ["e2e-tests"]
```

## Writing Tests

### Creating Feature Files

Feature files live in `app/crates/backend-server/tests/features/` and use standard Gherkin syntax:

```gherkin
# cuss_deposit.feature
Feature: CUSS Deposit Flow
  Complete flow for user registration and deposit through CUSS integration

  @serial
  Scenario: Successful CUSS deposit flow with metadata persistence
    Given a user with id "usr_test_001" exists
    And a CUSS mock server is running
    And the CUSS register endpoint returns success with fineractClientId 12345
    When the CHECK_USER_EXISTS step executes with session context:
      | phone_number  | +237690000000 |
      | full_name     | Test User     |
      | deposit_amount| 5000.0        |
    Then the CHECK_USER_EXISTS step completes with Done outcome
    And the user metadata contains fineractClientId 12345

  @serial
  Scenario: CUSS register retryable on 5xx errors
    Given a CUSS mock server is running
    And the CUSS register endpoint returns error with status 503
    When the CUSS_REGISTER_CUSTOMER step executes
    Then the CUSS_REGISTER_CUSTOMER step completes with Failed outcome
    And the failure is retryable
```

### Step Definition Patterns

Unlike traditional Cucumber, step definitions are pure Rust functions. Map Gherkin steps to Rust assertions:

```rust
// flow_cuss_e2e.rs
use wiremock::{Mock, MockServer, ResponseTemplate};
use wiremock::matchers::{method, path};
use serde_json::json;

#[tokio::test]
async fn test_flow_cuss_deposit_saves_metadata() {
    // Given: Setup mock server
    let mock_server = MockServer::start().await;
    let cuss_url = mock_server.uri();

    // And: Configure mock responses
    Mock::given(method("POST"))
        .and(path("/api/registration/register"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "success": true,
            "fineractClientId": 12345,
            "savingsAccountId": 67890
        })))
        .mount(&mock_server)
        .await;

    // When: Execute the flow
    let flow = CussDepositFlow::new(cuss_url);
    let outcome = register_step.execute(&ctx).await?;

    // Then: Verify outcomes
    match outcome {
        StepOutcome::Done { output, updates } => {
            assert_eq!(
                output.get("fineractClientId").and_then(|v| v.as_i64()),
                Some(12345)
            );
        }
        _ => panic!("Expected Done outcome"),
    }
}
```

### Best Practices for Organizing Features

1. **One feature file per flow/module**
   ```
   features/
   ├── cuss_deposit.feature      # CUSS integration
   ├── phone_otp.feature         # Phone OTP flow
   ├── email_magic.feature       # Email magic link flow
   └── first_deposit.feature     # First deposit flow
   ```

2. **Use `@serial` tag for tests that share state**
   ```gherkin
   @serial
   Scenario: Test that modifies database
   ```

3. **Group related scenarios with backgrounds**
   ```gherkin
   Feature: User Authentication
   
   Background:
     Given a valid Keycloak realm exists
     And the user storage service is running
   
   Scenario: Valid token grants access
   Scenario: Invalid token denies access
   ```

4. **Use data tables for complex inputs**
   ```gherkin
   When the step executes with context:
     | field         | value          |
     | phone_number  | +237690000000  |
     | deposit_amount| 5000.0         |
   ```

### Async Testing Considerations

**Tokio Runtime**: All E2E tests use `#[tokio::test]`:

```rust
#[tokio::test]
async fn test_async_flow_execution() {
    // Async setup
    let mock_server = MockServer::start().await;
    
    // Async execution
    let outcome = step.execute(&ctx).await?;
    
    // Async verification
    wait_for_condition(|| async {
        check_state().await
    }).await;
}
```

**Serial Execution**: Use `serial_test` crate for tests that cannot run in parallel:

```rust
use serial_test::file_serial;

#[tokio::test]
#[file_serial]
async fn test_modifies_shared_database() {
    // This test runs serially across the file
}
```

**Timeouts and Retries**: Handle async timing:

```rust
use tokio::time::{sleep, Duration, Instant};

async fn wait_for_otp(
    client: &reqwest::Client,
    env: &Env,
    phone: &str,
    timeout: Duration,
) -> Result<String> {
    let deadline = Instant::now() + timeout;
    let url = format!("{}/__admin/messages", env.sms_sink_url);

    while Instant::now() < deadline {
        let response = client.get(&url).send().await?;
        if let Some(otp) = extract_otp_from_response(&response, phone).await {
            return Ok(otp);
        }
        sleep(Duration::from_millis(500)).await;
    }
    Err(anyhow!("OTP not received within timeout"))
}
```

## Running Tests

### Commands to Run Tests

**Unit Tests** (fast, no external deps):
```bash
cargo test --workspace --lib
```

**Integration Tests** (OAS validation):
```bash
cargo test -p backend-server --features it-tests api::it_tests::
```

**Wiremock E2E Tests** (flow-level):
```bash
cargo test -p backend-server --features e2e-tests,flow-sdk --test flow_cuss_e2e
```

**Compose Smoke Tests** (quick health check):
```bash
just test-e2e-smoke
# or
cargo test -p backend-e2e --features e2e-tests --test smoke
```

**Compose Full E2E Tests** (comprehensive):
```bash
just test-e2e-full
# or
cargo test -p backend-e2e --features e2e-tests --test full
```

### Running Specific Features or Scenarios

**Run a single test file**:
```bash
cargo test -p backend-server --test flow_cuss_e2e -- --nocapture
```

**Run tests matching a pattern**:
```bash
cargo test -p backend-e2e --test full test_auth_enforcement -- --nocapture
```

**Run with verbose output**:
```bash
cargo test -p backend-server --features e2e-tests -- --nocapture --test-threads=1
```

### Integration with CI/CD

**GitHub Actions example**:
```yaml
name: E2E Tests

on: [push, pull_request]

jobs:
  smoke-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rust-lang/setup-rust-toolchain@v1
      - name: Run Smoke E2E
        run: just test-e2e-smoke

  full-tests:
    runs-on: ubuntu-latest
    needs: smoke-tests
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rust-lang/setup-rust-toolchain@v1
      - name: Run Full E2E
        run: just test-e2e-full
```

**Pre-commit hook example**:
```bash
#!/bin/bash
# .git/hooks/pre-commit

# Run quick checks
cargo test --workspace --lib || exit 1
cargo test -p backend-server --features it-tests || exit 1
```

## Maintenance

### Adding New Scenarios

1. **Add scenario to feature file**:
   ```gherkin
   # features/new_flow.feature
   Scenario: New flow happy path
     Given precondition
     When action occurs
     Then expected outcome
   ```

2. **Implement in Rust test**:
   ```rust
   #[tokio::test]
   async fn test_new_flow_happy_path() {
       // Given
       let mock_server = MockServer::start().await;
       setup_precondition(&mock_server).await;
       
       // When
       let result = execute_action(&mock_server).await;
       
       // Then
       assert_eq!(result.status, ExpectedStatus);
   }
   ```

3. **Add to test target** if needed:
   ```toml
   # Cargo.toml
   [[test]]
   name = "new_flow"
   path = "tests/new_flow.rs"
   required-features = ["e2e-tests"]
   ```

### Debugging Failing Tests

**1. Enable verbose output**:
```bash
cargo test --test flow_cuss_e2e -- --nocapture
```

**2. Check mock server interactions**:
```rust
// Add expect() to verify call count
Mock::given(method("POST"))
    .respond_with(ResponseTemplate::new(200))
    .expect(1)  // Will fail if called != 1 times
    .mount(&mock_server)
    .await;
```

**3. Log intermediate state**:
```rust
#[tokio::test]
async fn test_debug() {
    tracing_subscriber::fmt::init();
    
    let result = step.execute(&ctx).await?;
    tracing::debug!("Step outcome: {:?}", result);
}
```

**4. Isolate with single-threaded execution**:
```bash
cargo test -- --test-threads=1 --nocapture
```

**5. Check database state** (for Compose E2E):
```bash
docker compose -p user-storage-backend-e2e exec postgres psql -U postgres -d user-storage -c "SELECT * FROM sm_instance;"
```

### Common Patterns and Reusable Steps

**Mock Server Setup**:
```rust
// tests/common/mod.rs
pub async fn setup_cuss_mock_server() -> (MockServer, String) {
    let server = MockServer::start().await;
    let url = server.uri();
    (server, url)
}

pub async fn mock_successful_registration(server: &MockServer, client_id: i64) {
    Mock::given(method("POST"))
        .and(path("/api/registration/register"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "success": true,
            "fineractClientId": client_id
        })))
        .mount(server)
        .await;
}
```

**Test Context Builder**:
```rust
pub struct TestContext {
    pub session_id: String,
    pub flow_id: String,
    pub user_id: String,
}

impl TestContext {
    pub fn new(user_id: &str) -> Self {
        Self {
            session_id: format!("sess_{}", cuid::cuid().unwrap()),
            flow_id: format!("flow_{}", cuid::cuid().unwrap()),
            user_id: user_id.to_owned(),
        }
    }
    
    pub fn step_context(&self, input: Value) -> StepContext {
        StepContext {
            session_id: self.session_id.clone(),
            flow_id: self.flow_id.clone(),
            step_id: format!("step_{}", cuid::cuid().unwrap()),
            input,
            session_context: json!({ "userId": self.user_id }),
            flow_context: json!({}),
        }
    }
}
```

**Assertion Helpers**:
```rust
pub fn assert_step_done(outcome: &StepOutcome) -> &Value {
    match outcome {
        StepOutcome::Done { output, .. } => output.as_ref().unwrap(),
        _ => panic!("Expected Done outcome, got {:?}", outcome),
    }
}

pub fn assert_step_failed(outcome: &StepOutcome) -> (&str, bool) {
    match outcome {
        StepOutcome::Failed { error, retryable } => (error, *retryable),
        _ => panic!("Expected Failed outcome"),
    }
}
```

## Migration Notes

### Summary of Changes from Previous Approach

| Aspect | Previous | Current |
|--------|----------|---------|
| **Test Framework** | Plain `#[test]` | Gherkin specs + Rust implementation |
| **Documentation** | Code comments only | Living `.feature` files |
| **Mocking** | Manual trait impls | `wiremock` for HTTP, `mockall` for traits |
| **Async** | Block_on wrappers | Native `#[tokio::test]` |
| **Test Organization** | Mixed in source | Separate `tests/` directories |
| **CI Integration** | Manual scripts | `justfile` commands |

### How Existing Tests Were Converted

1. **Extract behavior to Gherkin**:
   ```rust
   // Before: Inline test
   #[test]
   fn test_register_success() {
       // ... test code
   }
   ```
   
   ```gherkin
   # After: Documented specification
   Scenario: CUSS register returns client ID
     Given a CUSS mock server is running
     When the register endpoint is called
     Then it returns fineractClientId
   ```

2. **Add wiremock for HTTP mocking**:
   ```rust
   // Before: Real HTTP calls or manual mocks
   let response = reqwest::get("http://real-server/api").await?;
   
   // After: Wiremock
   let mock_server = MockServer::start().await;
   Mock::given(method("GET"))
       .respond_with(ResponseTemplate::new(200).set_body_json(json!({...})))
       .mount(&mock_server)
       .await;
   let response = reqwest::get(&format!("{}/api", mock_server.uri())).await?;
   ```

3. **Use shared test utilities**:
   ```rust
   // Before: Duplicated setup code in each test
   let ctx = StepContext { /* 10 lines of setup */ };
   
   // After: Shared helper
   let ctx = TestContext::new("usr_001").step_context(json!({...}));
   ```

4. **Enable feature flags**:
   ```rust
   // Before: Always run (slow for unit test suite)
   #[test]
   fn test_external_service() { /* ... */ }
   
   // After: Feature-gated
   #[cfg(feature = "e2e-tests")]
   #[tokio::test]
   async fn test_external_service() { /* ... */ }
   ```

### Test File Reference

| Test File | Type | Feature File | Description |
|-----------|------|--------------|-------------|
| `flow_cuss_e2e.rs` | Wiremock | `cuss_deposit.feature` | CUSS integration flow |
| `smoke.rs` | Compose | - | Service health checks |
| `full.rs` | Compose | - | Full stack scenarios |
| `oidc_wiremock_e2e.rs` | Wiremock | - | OIDC authentication |
| `it_tests.rs` | Integration | - | OpenAPI spec validation |

## Quick Reference

### Test Commands

```bash
# Unit tests
cargo test --workspace --lib

# Integration tests
just test-it

# Flow E2E (wiremock)
cargo test -p backend-server --features e2e-tests,flow-sdk --test flow_cuss_e2e

# Smoke E2E (compose)
just test-e2e-smoke

# Full E2E (compose)
just test-e2e-full

# Debug failing test
cargo test --test full -- --nocapture --test-threads=1
```

### Key Files

- **Feature specs**: `app/crates/backend-server/tests/features/*.feature`
- **Flow tests**: `app/crates/backend-server/tests/flow_cuss_e2e.rs`
- **E2E tests**: `app/crates/backend-e2e/tests/{smoke,full}.rs`
- **Shared utilities**: `app/crates/backend-e2e/tests/common/mod.rs`
- **Justfile commands**: `justfile` (lines 102-176)

### Dependencies

```toml
wiremock = "0.6.5"      # HTTP mocking
tokio = "1"             # Async runtime
mockall = "0.14"        # Trait mocking
serial_test = "3.4.0"   # Test serialization
```