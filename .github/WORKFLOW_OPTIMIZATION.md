# GitHub Workflow Optimization Report

## Issue Reference
- GitHub Issue: #562
- Title: Optimize GitHub Workflows to Reduce Runner Costs

## Summary of Changes

This document outlines the optimizations made to GitHub Actions workflows to reduce unnecessary runner usage and associated costs while maintaining the same level of quality and automation.

---

## Changes to `.github/workflows/ci.yaml`

### 1. Path Filtering (High Impact)

**Before:**
```yaml
on:
  push:
  pull_request:
```

**After:**
```yaml
on:
  push:
    branches:
      - master
      - main
      - 'release/**'
      - 'hotfix/**'
    paths:
      - 'app/**'
      - 'src/**'
      - 'Cargo.toml'
      - 'Cargo.lock'
      - '.cargo/**'
      - 'config/**'
      - 'justfile'
      - 'openapi/**'
      - '.github/workflows/ci.yaml'
      - '.github/actions/**'
      - 'deploy/docker/**'
  pull_request:
    branches:
      - master
      - main
    paths:
      - 'app/**'
      - 'src/**'
      - 'Cargo.toml'
      - 'Cargo.lock'
      - '.cargo/**'
      - 'config/**'
      - 'justfile'
      - 'openapi/**'
      - '.github/workflows/ci.yaml'
      - '.github/actions/**'
      - 'deploy/docker/**'
```

**Impact:**
- ✅ Skips CI for documentation-only changes (README.md, docs/, etc.)
- ✅ Skips CI for non-code files (.gitignore, LICENSE, etc.)
- ✅ Only triggers on relevant file changes
- **Estimated savings: 30-50% reduction in workflow runs**

### 2. Branch Filtering (High Impact)

**Before:**
```yaml
on:
  push:  # Runs on ALL branches
```

**After:**
```yaml
on:
  push:
    branches:
      - master
      - main
      - 'release/**'
      - 'hotfix/**'
```

**Impact:**
- ✅ Only runs on important branches (master, main, release/*, hotfix/*)
- ✅ Skips CI for feature branches and WIP branches
- **Estimated savings: 20-30% reduction in workflow runs**

### 3. Concurrency Controls (High Impact)

**Added:**
```yaml
concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true
```

**Impact:**
- ✅ Cancels in-progress runs when new commits are pushed
- ✅ Prevents redundant workflow executions
- ✅ Reduces queue buildup on busy repositories
- **Estimated savings: 15-25% reduction in runner minutes**

### 4. Change Detection Job (Medium Impact)

**Added:**
```yaml
jobs:
  changes:
    name: Detect Changes 🔍
    runs-on: ubuntu-latest
    outputs:
      code: ${{ steps.filter.outputs.code }}
      rust: ${{ steps.filter.outputs.rust }}
      docker: ${{ steps.filter.outputs.docker }}
      openapi: ${{ steps.filter.outputs.openapi }}
    steps:
      - uses: actions/checkout@v6
      - uses: dorny/paths-filter@v3
        id: filter
        with:
          filters: |
            code:
              - 'app/**'
              - 'src/**'
              - 'Cargo.toml'
              - 'Cargo.lock'
              - '.cargo/**'
            rust:
              - '**/*.rs'
              - 'Cargo.toml'
              - 'Cargo.lock'
            docker:
              - 'deploy/docker/**'
              - '.github/workflows/ci.yaml'
            openapi:
              - 'openapi/**'
```

**Impact:**
- ✅ Enables conditional job execution based on file changes
- ✅ Skips unnecessary jobs when only specific files change
- **Estimated savings: 10-20% reduction in job executions**

### 5. Conditional Job Execution (Medium Impact)

**Test Jobs:**
```yaml
test-workspace:
  needs: changes
  if: |
    needs.changes.outputs.code == 'true' &&
    github.event.inputs.skip_tests != 'true'

test-oas:
  needs: changes
  if: |
    needs.changes.outputs.openapi == 'true' &&
    github.event.inputs.skip_tests != 'true'
```

**Build Jobs:**
```yaml
build-musl:
  needs: [changes, test-workspace, test-oas]
  if: |
    needs.changes.outputs.code == 'true' &&
    github.event.inputs.skip_build != 'true'
```

**Container Build Jobs:**
```yaml
container-build-dev:
  needs: [changes, build-musl, test-workspace, test-oas]
  if: |
    needs.changes.outputs.code == 'true' &&
    github.ref != 'refs/heads/master' &&
    always() &&
    (needs.test-workspace.result == 'success' || needs.test-workspace.result == 'skipped') &&
    (needs.test-oas.result == 'success' || needs.test-oas.result == 'skipped') &&
    (needs.build-musl.result == 'success')
```

**Impact:**
- ✅ Tests only run when code changes
- ✅ OAS tests only run when OpenAPI specs change
- ✅ Builds only run when code changes
- ✅ Containers only build when tests pass or are skipped
- **Estimated savings: 20-30% reduction in job executions**

### 6. Manual Workflow Dispatch (Low Impact)

**Added:**
```yaml
workflow_dispatch:
  inputs:
    skip_tests:
      description: 'Skip test jobs'
      required: false
      default: 'false'
      type: boolean
    skip_build:
      description: 'Skip build jobs'
      required: false
      default: 'false'
      type: boolean
```

**Impact:**
- ✅ Allows manual triggering with options to skip tests or builds
- ✅ Useful for emergency deployments or documentation updates
- **Estimated savings: Minimal direct impact, but improves operational flexibility**

---

## No Changes Required

### `.github/workflows/helm-release.yml`

**Status:** ✅ Already optimized

This workflow already has:
- Path filtering for `deploy/charts/user-storage/**`
- Manual dispatch option
- Tag-based triggers

### `.github/workflows/opencode.yml`

**Status:** ✅ Already optimized

This workflow already has:
- Conditional execution based on comment content
- Only runs when `/oc` or `/opencode` is mentioned

---

## Expected Cost Savings

### Conservative Estimates

| Optimization | Estimated Savings | Impact Level |
|-------------|-------------------|--------------|
| Path filtering | 30-50% | High |
| Branch filtering | 20-30% | High |
| Concurrency controls | 15-25% | High |
| Change detection | 10-20% | Medium |
| Conditional execution | 20-30% | Medium |
| **Combined effect** | **60-80%** | **Very High** |

### Monthly Runner Minutes (Example)

**Before optimization:**
- Average runs per day: 20-30
- Average duration: 15-20 minutes
- Monthly minutes: 9,000-18,000 minutes

**After optimization:**
- Average runs per day: 5-10
- Average duration: 15-20 minutes
- Monthly minutes: 2,250-6,000 minutes

**Potential savings: 6,000-12,000 runner minutes per month**

---

## Testing Recommendations

### 1. Verify Path Filtering

```bash
# Test 1: Push to README.md (should NOT trigger CI)
git commit --allow-empty -m "docs: update README"
git push origin optimize-github-workflows-runner-costs

# Test 2: Push to app/crates/backend-core/src/lib.rs (should trigger CI)
# Make a small change and push

# Test 3: Push to .github/workflows/ci.yaml (should trigger CI)
# Make a small change and push
```

### 2. Verify Branch Filtering

```bash
# Test 1: Push to master branch (should trigger CI)
# Test 2: Push to feature branch (should NOT trigger CI)
# Test 3: Push to release/v1.0.0 branch (should trigger CI)
```

### 3. Verify Concurrency Controls

```bash
# Test 1: Push multiple commits rapidly
# Test 2: Verify only the latest commit runs
# Test 3: Verify previous runs are cancelled
```

### 4. Verify Conditional Execution

```bash
# Test 1: Change only openapi/*.yaml files
# Expected: Only test-oas job runs, test-workspace skipped

# Test 2: Change only README.md
# Expected: No CI jobs run

# Test 3: Change app/crates/backend-core/src/lib.rs
# Expected: All CI jobs run
```

### 5. Verify Manual Dispatch

```bash
# Test 1: Trigger workflow with skip_tests=true
# Expected: Tests are skipped, builds run

# Test 2: Trigger workflow with skip_build=true
# Expected: Tests run, builds are skipped
```

---

## Rollback Plan

If issues arise, the workflow can be reverted to the previous version:

```bash
git revert <commit-hash>
git push origin master
```

Or manually restore the original triggers:

```yaml
on:
  push:
  pull_request:
```

---

## Monitoring Recommendations

### 1. Track Workflow Runs

Monitor the number of workflow runs per day/week to verify reduction:

```bash
gh run list --workflow=ci.yaml --limit 100
```

### 2. Track Runner Minutes

Use GitHub's billing/usage dashboard to track runner minutes consumed.

### 3. Track Job Execution

Monitor which jobs are skipped vs executed:

```bash
gh run view <run-id> --log
```

---

## Acceptance Criteria Status

- [x] All repositories have been reviewed for workflow optimization opportunities
- [x] Unnecessary workflow triggers have been identified and removed
- [x] Workflows only execute when relevant changes occur
- [x] Duplicate or redundant pipeline executions have been eliminated
- [x] Branch triggers have been reviewed and restricted where appropriate
- [x] Path-based filtering has been implemented where applicable
- [x] Expensive jobs are skipped when their execution is not required
- [ ] Existing CI/CD functionality remains intact (requires testing)
- [ ] Build, test, security, and deployment workflows continue to run when needed (requires testing)
- [x] Documentation of workflow changes and expected cost savings has been provided

---

## Next Steps

1. **Test the workflow changes** on this branch
2. **Monitor workflow execution** for 1-2 weeks
3. **Compare runner usage** before and after optimization
4. **Adjust path filters** if needed based on real-world usage
5. **Consider additional optimizations**:
   - Matrix build optimization for dev builds
   - Scheduled security scans
   - Dependency caching improvements

---

## References

- [GitHub Actions Workflow Syntax](https://docs.github.com/en/actions/reference/workflow-syntax-for-github-actions)
- [Events that trigger workflows](https://docs.github.com/en/actions/reference/events-that-trigger-workflows)
- [Using concurrency](https://docs.github.com/en/actions/learn-github-actions/workflow-syntax-for-github-actions#concurrency)
- [Path filter pattern cheat sheet](https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions#filter-pattern-cheat-sheet)