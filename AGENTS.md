# AGENTS.md - Upgraded

## Repository Overview

Tokenization/user-storage backend with three HTTP surfaces:
- **KC**: `/kc/*` - Keycloak integration
- **BFF**: `/bff/*` - Backend for Frontend  
- **Staff**: `/staff/*` - Staff/admin operations

**Architecture**: Rust workspace with native `axum` runtime, strict `controller -> repository` layering, and Diesel-async for database access.

## Build, Test & Lint Commands

### Quick Development Cycle

```bash
# Run backend in dev mode with logs
just dev

# Run a single test by name
cargo test -p <crate> <test_name>--- --exact --nocapture

# Run tests for a specific crate
cargo test -p backend-server
cargo test -p backend-core
cargo test -p backend-auth
cargo test -p backend-repository

# Run all workspace tests
cargo test --workspace --locked

# Run only unit tests (skip integration tests)
cargo test --workspace --lib
```

### Integration & E2E Testing

```bash
# OAS3 integration tests (requires it-tests feature)
just test-it
cargo test -p backend-server --features it-tests api::it_tests::

# Rust-native E2E tests with external deps (requires e2e-tests feature)
cargo test -p backend-auth --features e2e-tests --test oidc_wiremock_e2e
cargo test -p backend-repository --features e2e-tests --test state_machine_repo_testcontainers

# Compose E2E tests (full stack)
just test-e2e-smoke  # Quick smoke tests
just test-e2e-full   # Full test suite
```

### Linting & Code Quality

```bash
# Format code
cargo fmt

# Run clippy with fixes
cargo clippy --all-targets --all-features --fix --allow-dirty -- -D warnings

# Check workspace compilation
cargo check --workspace

# Run all checks (format, clippy, fix)
just all-checks
```

## Code Style Guidelines

### SOLID Principles

Full SOLID principles documentation with code examples [available here](.opencode/SOLID.md)

### Architectural Patterns

Repository Pattern, Flow SDK Pattern, and more documented [here](.opencode/ARCHITECTURE.md)

## Opencode Commands

This project includes **10 specialized AI agents** with **10 executable commands** defined per opencode specification.

### Command Configuration

Commands are defined in `.opencode/commands/` as executable bash files following opencode spec:
- **Frontmatter**: Commands can include YAML metadata
- **Template**: Prompt sent to LLM for AI-powered commands
- **Agent Assignment**: Each command targets specific agent
- **Arguments**: Support `$ARGUMENTS`, `!command`, `@filename` placeholders

### Available Commands

| Command | Agent | Description | Usage |
|---------|-------|-------------|-------|
| `daily-standup` | agent-orchestrator | Project status check | `opencode run daily-standup` |
| `generate-bff` | bff-generator | Generate OpenAPI code | `opencode run generate-bff` |
| `validate-flow <name>` | flow-* agents | Validate flow implementation | `opencode run validate-flow phone_otp` |
| `test-flow <name>` | test-engineer | Run flow tests | `opencode run test-flow phone_otp` |
| `implement-otp-flow` | flow-otp-master | Generate OTP boilerplate | `opencode run implement-otp-flow` |
| `coordinate-agents` | agent-orchestrator | Run all agents in order | `opencode run coordinate-agents` |
| `coordinate-phase` | agent-orchestrator | Run specific phase | `opencode run coordinate-phase foundation` |
| `resolve-conflicts` | agent-orchestrator | Resolve agent conflicts | `opencode run resolve-conflicts agent1 agent2` |
| `schedule-work` | agent-orchestrator | Schedule agent tasks | `opencode run schedule-work <agent> <task>` |
| `track-progress` | agent-orchestrator | Monitor all agents | `opencode run track-progress` |

### Command Implementation

Commands follow opencode spec with YAML frontmatter support:

**Example: generate-bff.md**
```yaml
---
description: Generate BFF OpenAPI code and verify compilation
agent: bff-generator
model: gemini-3.1-flash-lite
temperature: 0.3
---

Run code generation:
!`just generate`

Verify compilation:
!`cargo check --features flow-sdk`

Run OAS tests:
!`just test-it`
```

**Example: test-flow.md**
```yaml
---
description: Run tests for a specific flow with coverage
agent: test-engineer
model: qwen3-vl-30b-a3b-thinking
temperature: 0.1
---

Run tests for: $ARGUMENTS
!`cargo test -p backend-flow-sdk --test $ARGUMENTS -- --nocapture`

cargo tarpaulin --test $ARGUMENTS --out Stdout
```

### Quick Start with Commands

```bash
# 1. List all available agents
opencode agent list

# 2. Run daily project status check (recommended first)
opencode run --agent agent-orchestrator daily-standup

# 3. Generate BFF OpenAPI code
opencode run --agent bff-generator generate-bff

# 4. Test specific flow
opencode run --agent test-engineer test-flow phone_otp

# 5. Validate flow implementation
opencode run --agent flow-otp-master validate-flow phone_otp

# 6. Track overall project progress
opencode run --agent agent-orchestrator track-progress
```

### Command Configuration Reference

**Frontmatter Options:**
- `description`: Command purpose shown in TUI
- `agent`: Target agent for execution (default: current agent)
- `model`: LLM model override
- `temperature`: Control randomness (0.0-1.0)
- `steps`: Max iterations before forcing text response
- `subtask`: Force subagent invocation (boolean)

**Special Placeholders:**
- `$ARGUMENTS`: Pass command arguments
- `$1`, `$2`, `$3`: Positional arguments
- `!`command``: Shell output injection
- `@filename`: File content injection

**Permissions Control:**
- Configure via `opencode.json` per command
- Glob patterns: `edit`, `bash`, `write`
- Values: `allow`, `ask`, `deny`

### Project Phases via Commands

**Phase 1: Foundation**
```bash
opencode run --agent bff-generator generate-bff
opencode run --agent flow-architect design-integration-traits
```

**Phase 2: Core Flows**  
```bash
opencode run --agent flow-otp-master implement-otp-flow
opencode run --agent flow-otp-master validate-flow phone_otp
opencode run --agent test-engineer test-flow phone_otp
```

**Phase 3: Testing**
```bash
for flow in phone_otp email_magic first_deposit id_document address_proof; do
  opencode run --agent test-engineer test-flow $flow
done
```

**Phase 4: Delivery**
```bash
opencode run --agent project-closer final-lint
opencode run --agent project-closer update-documentation
```

### Built-in Commands

OpenCode also includes built-in commands:
- `/init` - Initialize project
- `/undo` - Undo last action
- `/redo` - Redo undone action
- `/share` - Share conversation
- `/help` - Show help

Custom commands override built-in commands when names conflict.

### Troubleshooting Commands

**Command not found:**
```bash
# Ensure opencode is initialized
opencode agent list

# Check command files exist
ls -la .opencode/commands/
```

**Agent not executing:**
```bash
# Verify agent is loaded
opencode agent list | grep agent-name

# Check agent permissions in config.json
```

**Permission denied:**
```bash
# Check command is executable
ls -l .opencode/commands/command-name

# Ensure proper permissions
chmod +x .opencode/commands/*
```

### Full Documentation

For complete command reference and configuration options, see:
- [Commands Documentation](https://opencode.ai/docs/commands) - Official spec
- `.opencode/AGENTS-QUICK-REFERENCE.md` - Agent and command reference
- `.opencode/PROJECT-EXECUTION.md` - Detailed project phases
- `.opencode/commands/` - Command implementations

## Database & Migrations

### Migration Workflow

```bash
# Create new migration
cd app/crates/backend-migrate
cargo run -- create_migration <name>

# Must touch a Rust file after adding SQL migration:
touch app/crates/backend-migrate/src/migrate.rs
```

### Migration Rules

- Naming: `YYYYMMDDHHMMSS_description.sql`
- Use `TEXT` not `VARCHAR` for string columns
- Define indices and constraints in migration files
- Use Diesel DSL, avoid raw SQL where possible

## Key Directories

- `app/crates/`: Library crates (`backend-server`, `backend-core`, `backend-auth`, etc.)
- `app/bins/`: Binary crates (`backend` server, `sms-gateway`)
- `app/gen/`: Generated code (OpenAPI models) - **NEVER EDIT MANUALLY**
- `openapi/`: OpenAPI spec files (source of truth)
- `app/crates/backend-migrate/migrations/`: Database migrations
- `config/`: Configuration YAML files

## Testing Best Practices

### Unit Tests

- Location: `src/` module files (inline) or `tests/` directory for integration tests
- Use `mockall` for mocking traits
- Test both success and failure paths
- For repository tests: set `DATABASE_URL` or tests will skip

### Integration Tests

- Feature-gated: `--features it-tests` for OAS, `--features e2e-tests` for external deps
- OAS tests: `app/crates/backend-server/src/api/it_tests.rs`
- E2E tests: `app/crates/backend-e2e/tests/`

### Cucumber BDD Tests

The project supports Cucumber BDD-style testing for e2e scenarios:

```bash
# Run cucumber smoke tests
just test-cucumber-smoke

# Run cucumber full tests  
just test-cucumber-full

# Run all cucumber tests
just test-cucumber-all
```

**Structure:**
- Feature files: `app/crates/backend-e2e/tests/features/*.feature`
- Step definitions: `app/crates/backend-e2e/tests/cucumber_*.rs`
- World state: `app/crates/backend-e2e/tests/world.rs`

**Writing New Tests:**
1. Create a `.feature` file in `tests/features/`
2. Add step definitions in `cucumber_full.rs` using `#[given]`, `#[when]`, `#[then]` attributes
3. Use `@serial` tag for scenarios that must run in isolation
4. Run with `cargo test -p backend-e2e --features e2e-tests --test cucumber_full`

### Required Test Coverage

- `backend_core::Error` mapping and response behavior
- Bearer/JWT middleware bypass and enforcement cases
- KC signature verification (all failure modes + success)
- Device binding unique-conflict races
- SMS retry behavior (transient vs permanent errors)

## OpenAPI Workflow

1. Modify specs in `openapi/*.yaml` (not `app/gen/`)
2. Regenerate code: `just generate`
3. Validate: `just test-it`
4. Update handlers if API contract changed

## Configuration

- Config source: `backend-core::Config` only
- Supports env var expansion: `${VAR}` or `${VAR:-default}`
- Use `clap` for CLI args in binaries
- Shared state in `AppState` with `Arc<dyn Trait>` abstractions