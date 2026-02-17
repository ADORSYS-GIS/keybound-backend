# Architecture Plan: Refactor Tiers, Config, and Staff Query

## 1. Calculated Tiers

### Context
Currently, the KYC tier is explicitly stored in `kyc_case.current_tier` and `kyc_submission.requested_tier`/`decided_tier`. The requirement is to remove these persisted fields and calculate the tier dynamically based on the user's KYC state (specifically, the provided and approved documents).

### Changes

#### Database Schema
1.  **Migration**: Create a new migration `app/crates/backend-migrate/migrations/YYYYMMDDHHMMSS_remove_explicit_tiers.sql`.
    *   `ALTER TABLE kyc_case DROP COLUMN current_tier;`
    *   `ALTER TABLE kyc_submission DROP COLUMN requested_tier;`
    *   `ALTER TABLE kyc_submission DROP COLUMN decided_tier;`

#### Domain Logic (Tier Calculation)
Since the tier is no longer stored, it must be derived. We will define a `TierCalculator` (or logic within `KycRepo`) that determines the tier based on **Approved** documents.

*   **Logic Proposal**:
    *   **Tier 0**: Default / No approved submission.
    *   **Tier 1**: Has an approved submission with at least one valid "Identity" document (e.g., `doc_type = 'PASSPORT'` or `'ID_CARD'`).
    *   **Tier 2**: Has an approved submission with "Identity" AND "Address" documents (e.g., `doc_type = 'UTILITY_BILL'`).
    *   *Note*: This logic can be adjusted in implementation, but the architecture relies on "Tier = f(Approved Documents)".

#### Repository Layer (`backend-repository`)
1.  **Update `KycRepo::get_kyc_tier`**:
    *   Instead of `SELECT current_tier FROM kyc_case`, it must:
        1.  Find the latest `APPROVED` submission for the user.
        2.  Fetch the documents associated with that submission.
        3.  Apply the calculation logic.
    *   If no approved submission exists, return `Tier 0`.
2.  **Update `KycRepo::ensure_kyc_profile`**:
    *   Remove `requested_tier` from the `INSERT INTO kyc_submission` statement.
3.  **Update `KycRepo::update_kyc_approved`**:
    *   Remove `decided_tier` update in `kyc_submission`.
    *   Remove `current_tier` update in `kyc_case`.

#### API Layer (`backend-server`)
*   Update `api/bff.rs` and `api/staff.rs` to remove references to `requested_tier` or `decided_tier` in response DTOs if they are merely echoing the DB columns. If the API contract requires them, populate them using the calculated value.

---

## 2. Env Var Expansion in Config

### Context
The current configuration loader (`backend-core/src/config.rs`) reads a YAML file directly. We need to support environment variable expansion (e.g., `${DATABASE_URL}`) before parsing.

### Changes

#### `backend-core`
1.  **Modify `load_from_path` in `src/config.rs`**:
    *   Read the file content as a string.
    *   Pass the string through a new function `expand_env_vars(content: &str) -> Result<String>`.
    *   Parse the expanded string with `serde_yaml`.

2.  **Implement `expand_env_vars`**:
    *   Use a regex (e.g., `\$\{([a-zA-Z_][a-zA-Z0-9_]*)\}`) to identify variables.
    *   For each match, look up the variable in `std::env::var`.
    *   **Behavior**:
        *   If found: Replace with value.
        *   If not found: Return an error (fail fast) OR leave as-is (depending on preference, fail-fast is usually safer for config). *Decision: Fail fast with a clear error message.*
   *    First look for a crates that can help do this job before going for manual in-house implementation 

---

## 3. Optimized Staff Query

### Context
`api_kyc_staff_submissions_get` currently fetches **all** submissions into memory and filters them in Rust. This is inefficient. We will push filtering and pagination to the database.

### Changes

#### Repository Layer (`backend-repository`)
1.  **Define Filter Struct**:
    ```rust
    pub struct KycSubmissionFilter {
        pub status: Option<String>,
        pub search: Option<String>, // Matches first_name, last_name, email
        pub limit: i64,
        pub offset: i64,
    }
    ```
2.  **Update `list_kyc_submissions`**:
    *   Change signature to `async fn list_kyc_submissions(&self, filter: KycSubmissionFilter) -> RepoResult<(Vec<db::KycSubmissionRow>, i64)>`.
    *   Return both the data and the total count (for pagination).
    *   Use `diesel::QueryDsl::into_boxed`:
        *   Apply `status.eq(...)` if present.
        *   Apply `(first_name.ilike(%search%).or(last_name.ilike(%search%))...)` if search is present.
        *   Apply `.limit()` and `.offset()`.

#### API Layer (`backend-server`)
1.  **Update `api/staff.rs`**:
    *   Extract `status`, `search`, `page`, `limit` from `query_params`.
    *   Construct `KycSubmissionFilter`.
    *   Call the updated `list_kyc_submissions`.
    *   Map the results directly to the response DTOs.

---

## Execution Plan

1.  **Step 1: Config Expansion**
    *   Implement env var expansion in `backend-core`.
    *   Verify with a test case.

2.  **Step 2: Staff Query Optimization**
    *   Refactor `KycRepo::list_kyc_submissions` to accept filters.
    *   Update `backend-server` to use the new signature.
    *   Verify `api_kyc_staff_submissions_get` works with filters.

3.  **Step 3: Tier Refactoring**
    *   Create the migration to drop columns.
    *   Implement `calculate_tier` logic in `backend-repository`.
    *   Update `get_kyc_tier` and other repo methods.
    *   Fix compilation errors in `backend-server` due to missing fields.
    *   Run migration and verify.
