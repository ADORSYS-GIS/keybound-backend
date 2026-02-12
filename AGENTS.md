# AGENTS.md

## 1. Project Purpose
This workspace implements a tokenization/user-storage backend with 3 OA3 server surfaces:
- `KC` (`/v1/*`)
- `BFF` (`/api/registration/*`)
- `Staff` (`/api/kyc/staff/*`)

Binary entrypoint is `app/backend`. HTTP server implementation is `crates/backend-server` (library crate).

## 2. Architecture We Follow (SOLID + MVC-style layering)

### 2.1 Layer boundaries
1. **Controllers**
   - `crates/backend-server/src/api.rs`
   - Implements generated `Api<C>` traits and only handles transport + response shaping.
2. **Services**
   - `crates/backend-server/src/services.rs`
   - Orchestrates business logic and talks to repositories.
3. **Repositories**
   - `crates/backend-repository/*`
   - Owns SQL and persistence concerns.

Flow is always:
`controller -> service -> repository`

### 2.2 Workspace crate roles
- `app/backend`: CLI (`serve`, `migrate`, `config`)
- `backend-core`: source of truth for config + shared error/result types
- `backend-server`: axum host, controller/service wiring
- `backend-model`: DB row structs (`FromRow`) + DTO mapping (`o2o`)
- `backend-repository`: repository traits + PostgreSQL implementation
- `backend-auth`: generated service context/auth compatibility
- `backend-id`: prefixed CUID identifiers
- `backend-migrate`: migration runner
- `gen_oas_*`: generated OA3 artifacts (never hand-edit)

## 3. Non-Negotiable Rules

1. **Never edit `crates/gen_*` manually.**
2. API contract changes happen in `openapi/*` + regeneration.
3. Keep `backend-server` as a **library crate**; start it from `app/backend`.
4. Never mix SQL with controller/service Rust code.
5. Use repository traits (`backend-repository/src/traits.rs`) as boundaries.
6. Never use UUIDs for backend IDs; use prefix + CUID helpers from `backend-id`.
7. Do not create custom error hierarchies for app/repo layers; use `backend_core::Error`.
8. Do not add config mirrors in `backend-server`; extend `backend-core` config only.

## 4. ID Policy (Mandatory)

Use prefixed CUID IDs:
- `usr_*` users
- `dvc_*` device records
- `apr_*` approvals
- `sms_*` SMS challenge hash IDs

Helper APIs:
- `backend_id::user_id()`
- `backend_id::device_id()`
- `backend_id::approval_id()`
- `backend_id::sms_hash()`

## 5. Device Binding Safety

Enforce uniqueness for key material on **both** `device_id` and `jkt`:
- precheck for UX guidance
- bind-time recheck for race/direct-call safety
- handle uniqueness conflicts by reloading owner and returning deterministic conflict behavior

## 6. OpenAPI + Codegen Workflow

1. Edit relevant `openapi/*.json`.
2. Regenerate:
   - `docker compose -f compose.yml run --rm generate-code`
3. Validate:
   - `cargo check --workspace`
4. Never patch generated outputs manually.

## 7. Pagination Contract Rules

For list-like/read-many endpoints, always expose pagination in OpenAPI:
- query params: `page`, `limit`
- response metadata where relevant: `page`, `pageSize`, totals

Current paginated surfaces include:
- BFF `GET /api/registration/kyc/status` (documents payload pagination metadata)
- Staff `GET /api/kyc/staff/submissions`
- Staff `GET /api/kyc/staff/submissions/{externalId}` (documents payload pagination metadata)

## 8. Repository + SQLx-Data Rules

- Use `sqlx-data` utilities for repository pagination flows (`Serial`, `ParamsBuilder`, etc.).
- Keep raw SQL only in `backend-repository`.
- Repository methods should be reusable and side-effect explicit.

## 9. Mapping Rules (`o2o`)

- Use `o2o` for generated DTO boundary mapping.
- Keep DB row models in `backend-model/src/db.rs` with `sqlx::FromRow`.
- Prefer declarative `o2o` mapping; only do manual mapping when fields need custom parsing/transforms.

## 10. Caching Strategy

- In-process caching uses **LRU** (not `moka`).
- HTTP-ish read caches live in `backend-server` state.
- Repository-level small hot-path cache is allowed (currently user-by-phone).
- If distributed/shared caching is needed, use Redis.

## 11. Redis + Compose

Compose includes Redis without auth:
- service defined in `compose/redis.compose.yml`
- included by root `compose.yml`
- image: `redis:latest`
- port: `6379`

## 12. Config Model

`backend_core::Config` is the only config source of truth.
Primary local file: `config/default.yaml`

Main sections:
- `server.api.*`
- `database.*`
- `oauth2.*`
- `aws.*`

## 13. Migration Notes

There are two migration naming formats for the same migration content:
- `migrations/2026-02-03-000001_init_authz/{up.sql,down.sql}`
- `migrations/20260203000001_init_authz.{up.sql,down.sql}`

Keep both copies synchronized until migration format cleanup is explicitly performed.

## 14. Local Development Commands

- Start infra:
  - `just up-single postgres`
  - `just up-single redis`
  - `just up-single keycloak-26`
- Build/check:
  - `cargo check --workspace`
- Migrate:
  - `just dev migrate -c config/default.yaml`
- Serve:
  - `just dev serve -c config/default.yaml`

## 15. Change Checklist

Before finalizing a change:
1. `cargo check --workspace` passes.
2. If API changed, OpenAPI is updated and codegen re-run.
3. No manual edits under `crates/gen_*`.
4. Controller-service-repository layering is respected.
5. SQL remains in repositories only.
6. Error handling uses `backend_core::Error`.
