# Cucumber BDD Testing Guide

## Overview

This project uses Cucumber (Behavior-Driven Development) for end-to-end testing. Cucumber tests are written in Gherkin syntax (`.feature` files) with Rust step definitions, providing executable documentation that bridges the gap between technical implementation and business requirements.

### Why Cucumber?

| Aspect | Traditional Tests | Cucumber BDD |
|--------|-------------------|--------------|
| Documentation | Separate from code | Living documentation in `.feature` files |
| Readability | Code-centric | Natural language, stakeholder-friendly |
| Maintenance | Scattered assertions | Centralized step definitions |
| Collaboration | Developer-focused | Cross-team collaboration |

### Testing Pyramid

```
        ┌─────────┐
        │   E2E   │  ← Cucumber (Full Stack)
        │  Tests  │
        ├─────────┤
        │Integration│ ← OAS Tests, Wiremock
        │   Tests   │
        ├─────────┤
        │  Unit    │  ← Module tests
        │  Tests   │
        └─────────┘
```

## Setup Guide

### Dependencies

The following dependencies are required in `Cargo.toml`:

```toml
[dev-dependencies]
cucumber = "0.22"
tokio = { version = "1", features = ["macros", "rt-multi-thread", "time"] }
futures = "0.3"
wiremock = "0.6"
reqwest = { version = "0.12", features = ["json"] }
anyhow = "1"
serde_json = "1"

[[test]]
name = "cucumber_smoke"
harness = false

[[test]]
name = "cucumber_full"
harness = false
```

### Directory Structure

```
app/crates/backend-e2e/
├── Cargo.toml
├── tests/
│   ├── cucumber_smoke.rs    # Smoke test runner
│   ├── cucumber_full.rs     # Full E2E test runner
│   ├── world.rs             # Shared test state and helpers
│   └── features/
│       ├── smoke.feature     # Health check scenarios
│       ├── auth.feature      # Authentication scenarios
│       ├── bff_deposits.feature  # Deposit and OTP flow
│       ├── sms.feature       # SMS error handling
│       └── user.feature      # User endpoint scenarios
```

## Writing Tests

### Feature Files (Gherkin Syntax)

Feature files describe behavior in natural language:

```gherkin
Feature: User Authentication
  Verify BFF authentication behavior

  Background:
    Given the e2e test environment is initialized
    And I have a valid authentication token

  Scenario: Unauthenticated request is rejected
    When I send a GET request to /bff/internal/users/me without authentication
    Then the response status is 401

  Scenario Outline: Multiple auth schemes are validated
    When I send a POST request to <path> with <auth_type>
    Then the response status is <expected_status>

    Examples:
      | path           | auth_type      | expected_status |
      | /bff/sessions  | invalid Bearer | 401             |
      | /staff/users   | Basic auth     | 401             |
      | /bff/kyc       | no auth        | 401             |
```

### Step Definitions (Rust)

Step definitions connect Gherkin steps to code:

```rust
use cucumber::{given, then, when, World};

#[derive(Debug, World)]
#[world(init = Self::new)]
pub struct TestWorld {
    client: Option<reqwest::Client>,
    token: Option<String>,
    last_response: Option<JsonResponse>,
}

impl TestWorld {
    async fn new() -> Result<Self, anyhow::Error> {
        Ok(Self {
            client: Some(http_client()?),
            ..Default::default()
        })
    }
}

// GIVEN steps - Setup preconditions
#[given("the e2e test environment is initialized")]
async fn init_environment(world: &mut TestWorld) {
    match TestWorld::new().await {
        Ok(w) => {
            world.client = w.client;
        }
        Err(e) => {
            world.error = Some(e.to_string());
        }
    }
}

#[given("I have a valid authentication token")]
async fn get_auth_token(world: &mut TestWorld) {
    let token = obtain_test_token().await;
    world.token = Some(token);
}

// WHEN steps - Trigger actions
#[when(regex = r"^I send a (\w+) request to ([^\s]+) without authentication$")]
async fn send_request_no_auth(world: &mut TestWorld, method: String, path: String) {
    let response = world.client.as_ref().unwrap()
        .request(Method::from_str(&method).unwrap(), &format!("{}{}", BASE_URL, path))
        .send()
        .await
        .unwrap();
    
    world.last_response = Some(response.into());
}

// THEN steps - Assert outcomes
#[then(regex = r"^the response status is (\d+)$")]
async fn response_status_is(world: &mut TestWorld, expected: u16) {
    let response = world.last_response.as_ref().expect("No response");
    assert_eq!(response.status, expected);
}
```

### Step Pattern Matching

| Pattern Type | Syntax | Example |
|--------------|--------|---------|
| Exact match | `"literal text"` | `#[given("the database is clean")]` |
| Regex | `regex = "^pattern$"` | `#[when(regex = r"^I send a (\w+) request$")]` |
| Cucumber expression | `expr = "{word}"` | `#[given(expr = "a user {word} exists")]` |

### Best Practices

1. **Use Background** for common setup across scenarios
2. **Use Scenario Outline** for data-driven tests
3. **Tag scenarios** with `@serial` for tests that cannot run concurrently
4. **Keep steps atomic** - one action per step
5. **Reuse steps** across features when possible
6. **Use async** - all step functions are async by default

### Async Testing

```rust
#[when("I verify OTP with the received code")]
async fn verify_otp(world: &mut TestWorld) {
    let otp_code = world.stored_otp.as_ref().expect("No OTP received");
    
    // Async HTTP call
    let response = world.client.as_ref().unwrap()
        .post(&format!("{}/verify", BASE_URL))
        .json(&json!({ "code": otp_code }))
        .send()
        .await
        .unwrap();
    
    world.last_response = Some(response.into());
}
```

## Running Tests

### Quick Commands

```bash
# Run smoke tests (fast, basic validation)
just test-cucumber-smoke

# Run full E2E suite (requires running services)
just test-cucumber-full

# Run all cucumber tests
just test-cucumber-all
```

### Direct Cargo Commands

```bash
# Run smoke cucumber tests
cargo test -p backend-e2e --test cucumber_smoke -- --nocapture

# Run full cucumber tests
cargo test -p backend-e2e --features e2e-tests --test cucumber_full -- --nocapture

# Run specific feature file
cargo test -p backend-e2e --test cucumber_smoke -- -i "features/auth.feature"

# Run scenarios matching a pattern
cargo test -p backend-e2e --test cucumber_full -- -n "OTP"

# Run with tags
cargo test -p backend-e2e --test cucumber_full -- -t "@serial"

# Verbose output
cargo test -p backend-e2e --test cucumber_smoke -- -vv

# Fail fast on first error
cargo test -p backend-e2e --test cucumber_full -- --fail-fast
```

### CI/CD Integration

```yaml
# Example GitHub Actions
test-e2e:
  runs-on: ubuntu-latest
  services:
    postgres:
      image: postgres:15
    keycloak:
      image: quay.io/keycloak/keycloak:latest
  steps:
    - uses: actions/checkout@v4
    - run: just test-cucumber-smoke
    - run: just test-cucumber-full
      if: success()
```

## Maintenance

### Adding New Scenarios

1. Create or update a `.feature` file in `tests/features/`
2. Add scenario in Gherkin syntax
3. Run tests - cucumber will report missing step definitions
4. Implement step definitions in `cucumber_full.rs` or `cucumber_smoke.rs`

### Debugging Failing Tests

```bash
# Run with detailed output
cargo test -p backend-e2e --test cucumber_full -- --nocapture -vv

# Run single scenario
cargo test -p backend-e2e --test cucumber_full -- -n "Scenario name"

# Check test world state
# Add debug output in step definitions:
#[then("the response status is 200")]
async fn check_status(world: &mut TestWorld) {
    eprintln!("Debug: last_response = {:?}", world.last_response);
    // ... assertion
}
```

### Reusable Step Patterns

| Pattern | Step Text | Use Case |
|---------|-----------|----------|
| Setup | `Given the e2e test environment is initialized` | Initialize test world |
| Auth | `Given I have a valid authentication token` | Obtain test JWT |
| Request | `When I send a <method> request to <path>` | HTTP operations |
| Assert | `Then the response status is <code>` | Status validation |
| State | `Then <field> is <value>` | Response body checks |

## Migration Notes

### Changes from Previous Approach

| Aspect | Before | After |
|--------|--------|-------|
| Test format | Inline Rust tests | Gherkin feature files |
| Documentation | Comments in code | Living `.feature` specs |
| Reusability | Copy-paste assertions | Shared step definitions |
| Readability | Rust code | Natural language |

### Conversion Process

1. Extract test scenario from existing test function
2. Write as Gherkin scenario in `.feature` file
3. Create step definitions matching scenario steps
4. Reuse existing helper functions (e.g., `http_client`, `send_json`)
5. Move test world state to shared `world.rs`

### Test File Reference

| Feature | File | What it Tests |
|---------|------|---------------|
| Smoke | `smoke.feature` | Basic health checks |
| Auth | `auth.feature` | Authentication enforcement |
| BFF Deposits | `bff_deposits.feature` | OTP flow, deposit requests |
| SMS | `sms.feature` | SMS error handling |
| User | `user.feature` | KYC level, user endpoints |

## Troubleshooting

### Common Issues

**"Step not found"**
- Check step text matches exactly (including punctuation)
- Verify regex pattern syntax

**"No response recorded"**
- Ensure `When` step stores response in `world.last_response`
- Check for early returns on error

**"Test hangs"**
- Check for async operations without timeout
- Verify external services are running

**Compilation errors in regex**
- Use regular strings for patterns with quotes: `"^text \"value\"$"`
- Or use cucumber expressions: `expr = "text {string}"`