# Flow SDK Implementation Project

## 🎯 Project Overview
Complete the flow SDK implementation for the user-storage backend using collaborative AI agents.

## 📁 Structure

```
.opencode/
├── README.md              # General usage guide
├── config.json           # Master configuration
├── agents/               # 9 specialized agents
│   ├── flow-orchestrator.json
│   ├── bff-generator.json
│   ├── flow-architect.json
│   ├── flow-otp-master.json
│   ├── flow-email-wizard.json
│   ├── flow-deposit-builder.json
│   ├── integration-specialist.json
│   ├── test-engineer.json
│   └── project-closer.json
└── commands/             # 7 executable commands
    ├── daily-standup
    ├── generate-bff
    ├── validate-flow
    ├── implement-otp-flow
    ├── test-flow
    └── implement-otp-flow
```

## 🚀 Getting Started

```bash
# Initialize Opencode
opencode init

# List all agents
opencode agent list

# Run daily standup
opencode run --agent flow-orchestrator daily-standup

# View agent details
opencode agent show flow-orchestrator
```

## 🔧 Project Phases (Flexible Order)

Agents can work in parallel or sequence based on dependencies:

### Phase 1: Foundation & Code Generation
**When ready, run:**
```bash
# Generate BFF OpenAPI code
opencode run --agent bff-generator generate-bff
```

**Agent responsibilities:**
- `flow-architect`: Design integration traits (SmsProvider, EmailProvider, etc.)
- `flow-architect`: Create flow patterns documentation
- `bff-generator`: Generate and integrate OpenAPI code

**Deliverables:**
- Generated BFF handlers in `app/gen/oas_server_bff/`
- Integration trait definitions
- Flow pattern documentation

### Phase 2: Core Flow Implementation  
**After foundation is complete:**
```bash
# Generate OTP flow boilerplate
opencode run --agent flow-otp-master implement-otp-flow

# Validate flow implementation
opencode run --agent flow-otp-master validate-flow phone_otp
```

**Agent responsibilities:**
- `flow-otp-master`: Implement Phone OTP flow (IssuePhoneOtpStep, VerifyPhoneOtpStep)
- `flow-email-wizard`: Implement Email Magic flow
- `flow-otp-master`: Add rate limiting and retry logic
- `flow-email-wizard`: Create magic link generation and verification

**Deliverables:**
- Working Phone OTP flow with SMS integration
- Working Email Magic flow with secure tokens
- Flow registered in registry
- Unit tests for both flows

### Phase 3: Advanced Flows
**After core flows work:**
```bash
# Validate deposit flow
opencode run --agent flow-deposit-builder validate-flow first_deposit

# Validate document flows
opencode run --agent integration-specialist validate-flow id_document
```

**Agent responsibilities:**
- `flow-deposit-builder`: Implement First Deposit flow
- `flow-deposit-builder`: Integrate with CUSS client
- `integration-specialist`: Implement ID Document flow
- `integration-specialist`: Implement Address Proof flow
- `integration-specialist`: Create file upload integration

**Deliverables:**
- First Deposit flow with payment processing
- Document upload flows
- Admin verification workflows
- File storage integration

### Phase 4: Comprehensive Testing
**After flows are implemented:**
```bash
# Test specific flow
opencode run --agent test-engineer test-flow phone_otp

# Run all flow tests
for flow in phone_otp email_magic first_deposit id_document address_proof; do
    opencode run --agent test-engineer test-flow $flow
done
```

**Agent responsibilities:**
- `test-engineer`: Write unit tests for flow SDK core
- `test-engineer`: Create integration tests for orchestration
- `test-engineer`: Add OAS integration tests
- `test-engineer`: Run coverage reports (target: >80%)

**Deliverables:**
>80% test coverage for flow logic
All flows have unit + integration tests
E2E test scenarios documented

### Phase 5: Final Delivery
**After all flows and tests pass:**
```bash
# Full workspace validation
opencode run --agent flow-orchestrator check-workspace

# Final lint and documentation
opencode run --agent project-closer final-lint
opencode run --agent project-closer update-documentation
```

**Agent responsibilities:**
- `project-closer`: Run cargo fmt and clippy
- `project-closer`: Update AGENTS.md with implementation details
- `project-closer`: Complete all TODOs
- `project-closer`: Verify all tests pass
- `project-closer`: Create deployment configs

**Deliverables:**
Zero clippy warnings
Updated documentation
All tests passing
Production-ready codebase

## 📋 Available Commands

### Project Management
- `daily-standup`: Run project status check
- `check-workspace`: Full workspace validation (fmt, clippy, tests)

### Flow Development
- `validate-flow <name>`: Validate a flow implementation
- `test-flow <name>`: Run tests for a specific flow
- `implement-otp-flow`: Generate Phone OTP flow boilerplate

### Code Generation
- `generate-bff`: Generate BFF OpenAPI code

## 🤝 Collaboration Workflow

1. **Standup**: Run `daily-standup` regularly for status
2. **Feature Branches**: Work on branches named `agent/<name>/<feature>`
3. **Code Review**: All PRs require review from flow-orchestrator
4. **Merge Strategy**: Rebase onto `feat/flow-sdk-completed` branch
5. **Shared Branch**: `feat/flow-sdk-completed` is the integration branch

## 📊 Monitoring Progress

### Track Implementation Status
```bash
# View project status
opencode run --agent flow-orchestrator daily-standup

# Check feature completeness
for flow in phone_otp email_magic first_deposit id_document address_proof; do
    opencode run --agent flow-orchestrator validate-flow $flow
done
```

### Quality Gates
```bash
# Full validation
opencode run --agent flow-orchestrator check-workspace
```

## 🎯 Success Criteria

Run this to verify completion:

```bash
#!/bin/bash
echo "=== Flow SDK Project Completion Check ==="

# All flows validated
flows=("phone_otp" "email_magic" "first_deposit" "id_document" "address_proof")
for flow in "${flows[@]}"; do
    echo "Validating: $flow"
    opencode run --agent flow-orchestrator validate-flow "$flow"
done

# Code quality
opencode run --agent project-closer final-lint

# Integration tests
just test-it
just test-e2e-smoke

echo "=== Project Delivery Complete ==="
```

**Final Delivery Checklist:**
- [ ] All flows implemented with actual business logic
- [ ] BFF API code generated and integrated
- [ ] Test coverage >80% for flow logic
- [ ] Zero clippy warnings across workspace
- [ ] All tests pass: `cargo test --workspace --locked`
- [ ] OAS integration tests pass: `just test-it`
- [ ] E2E smoke tests pass: `just test-e2e-smoke`
- [ ] Documentation updated in AGENTS.md
- [ ] Deployment configs verified

## 🔧 Project Customization

### Add New Command
```bash
# Create new command file
touch .opencode/commands/my-command
chmod +x .opencode/commands/my-command
```

### Modify Agent LLM
Edit agent JSON file to change LLM models based on your needs.

## 📖 Help

```bash
# General help
opencode --help

# Command help
opencode run --help

# Agent help
opencode agent --help

# View command details
cat .opencode/commands/daily-standup
```

---

**Ready to start? Run:**
```bash
opencode run --agent flow-orchestrator daily-standup
```