---
description: Run tests for a specific flow with coverage report
agent: test-engineer
model: qwen3-vl-30b-a3b-thinking
temperature: 0.1
steps: 25
---

Run tests for flow: $ARGUMENTS

Execute: cargo test -p backend-flow-sdk --test $ARGUMENTS -- --nocapture

Generate coverage: cargo tarpaulin --test $ARGUMENTS --out Stdout

If coverage < 80%, identify gaps and suggest additional test cases.
