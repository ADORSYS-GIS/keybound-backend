---
description: Run daily project status check across all agents
agent: agent-orchestrator
model: gemini-2.5-pro
temperature: 0.2
steps: 20
---

Run comprehensive project status check:
!`git status --porcelain && git log -1 --pretty=format:'%s (%h)'`

Check compilation status:
!`cargo check --workspace --all-features`

List feature flags:
!`cd app/crates/backend-server && cargo tree --format "{p}" --edges features -e features 2>/dev/null | grep -E "flow-|step-" | sort | uniq`

Check for TODOs:
!`grep -r "TODO\|FIXME" app/crates/backend-flow-sdk/src --include="*.rs" | wc -l`

Provide summary of project health, blocking issues, and next steps.
