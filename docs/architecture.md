# Architecture

This project is a Rust workspace that exposes three OpenAPI (OA3) server surfaces (KC, BFF, Staff) using `axum`, with a strict MVC-style layering:

- **Controllers** (HTTP/OpenAPI boundary) live in `crates/backend-server`
- **Services** (business logic) live in `crates/backend-server`
- **Repositories** (SQL only) live in `crates/backend-repository`
- **DB models** (SQL row structs + DTO mapping) live in `crates/backend-model`
- **Config + errors + shared utilities** live in `crates/backend-core`

Generated crates in `crates/gen_*` are **never edited manually**; changes come from `openapi/*` + regeneration.

## Project Structure (Directories)

```mermaid
flowchart TB
  root["repo/"]
  root --> app["app/backend (binary)"]
  root --> crates["crates/ (workspace libs + generated)"]
  root --> openapi["openapi/*.json (OA3 sources)"]
  root --> migrations["migrations/*.sql (schema + changes)"]
  root --> compose["compose*.yml + compose/*.yml (runtime deps)"]
  root --> docs["docs/*.md"]

  crates --> core["backend-core"]
  crates --> server["backend-server"]
  crates --> repo["backend-repository"]
  crates --> model["backend-model"]
  crates --> idc["backend-id"]
  crates --> auth["backend-auth"]
  crates --> otlp["backend-otlp"]
  crates --> migrate["backend-migrate"]
  crates --> cli["backend-cli"]

  crates --> genKC["gen_oas_server_kc (generated)"]
  crates --> genBFF["gen_oas_server_bff (generated)"]
  crates --> genStaff["gen_oas_server_staff (generated)"]
  crates --> genClient["gen_oas_client_cuss_registration (generated)"]
```

## Runtime Request Flow (MVC)

```mermaid
sequenceDiagram
  autonumber
  participant C as Client
  participant A as axum Router (backend-server)
  participant G as gen_oas_server_* dispatch
  participant Ctrl as Controllers (backend-server/api.rs)
  participant Svc as Services (backend-server/services.rs)
  participant Repo as Repositories (backend-repository)
  participant DB as Postgres

  C->>A: HTTP request (KC/BFF/Staff)
  A->>G: Prefix dispatch (/v1, /api/registration, /api/kyc/staff)
  G->>Ctrl: Call generated Api trait impl
  Ctrl->>Svc: Validate + orchestrate
  Svc->>Repo: Persist/fetch (SQL)
  Repo->>DB: Queries (sqlx + sqlx-data)
  DB-->>Repo: Rows
  Repo-->>Svc: Domain structs
  Svc-->>Ctrl: DTOs
  Ctrl-->>C: HTTP response (generated DTOs)
```

## Crate Relationships (Layered)

```mermaid
flowchart LR
  subgraph App["app/backend (binary)"]
    BackendMain["main.rs"]
  end

  subgraph Server["crates/backend-server (library)"]
    Axum["axum + axum-server"]
    Controllers["Controllers (Api trait impls)"]
    Services["Services"]
    State["State + LRU caches"]
    Aws["AWS clients (S3/SNS)"]
  end

  subgraph Repo["crates/backend-repository"]
    RepoTraits["Repo traits"]
    PgRepo["Postgres impl (SQL only)"]
    SqlxData["sqlx-data (pagination/params)"]
  end

  subgraph Model["crates/backend-model"]
    DbRows["DB row structs (sqlx::FromRow)"]
    DtoMap["o2o mappings (gen DTO <-> domain/db)"]
  end

  subgraph Core["crates/backend-core"]
    Config["Config loader + schema"]
    Error["backend_core::Error"]
    Shared["shared utils (hashing, etc.)"]
  end

  subgraph Gen["crates/gen_* (generated)"]
    GenKC["gen_oas_server_kc"]
    GenBFF["gen_oas_server_bff"]
    GenStaff["gen_oas_server_staff"]
  end

  subgraph Infra["External"]
    PG["Postgres"]
    Redis["Redis (compose service)"]
    S3["S3-compatible storage"]
    SNS["SNS"]
  end

  BackendMain --> Config
  BackendMain --> Server
  BackendMain --> otlp["backend-otlp (tracing init)"]
  BackendMain --> migrate["backend-migrate (migrations)"]

  Server --> Axum
  Server --> Controllers
  Controllers --> Services
  Services --> Repo
  Repo --> PgRepo
  PgRepo --> PG

  Server --> Aws
  Aws --> S3
  Aws --> SNS

  Server --> Model
  Repo --> Model
  Model --> Gen
  Server --> Gen

  Server --> Core
  Repo --> Core
  Model --> Core
  idc["backend-id"] --> Core

  Redis -. optional .- Server
```

## Crates Node Graph (Workspace Dependency Sketch)

This is the intended dependency direction (lower layers never depend on upper layers):

```mermaid
flowchart TD
  Core["backend-core"]
  Id["backend-id"]
  Model["backend-model"]
  Repo["backend-repository"]
  Server["backend-server"]
  Otlp["backend-otlp"]
  Migrate["backend-migrate"]
  Cli["backend-cli"]
  App["app/backend"]

  GenKC["gen_oas_server_kc"]
  GenBFF["gen_oas_server_bff"]
  GenStaff["gen_oas_server_staff"]

  Id --> Core
  Model --> Core
  Repo --> Core
  Repo --> Id
  Repo --> Model
  Server --> Core
  Server --> Id
  Server --> Model
  Server --> Repo
  Server --> GenKC
  Server --> GenBFF
  Server --> GenStaff
  Model --> GenKC
  Model --> GenBFF
  Model --> GenStaff

  Otlp --> Core
  Migrate --> Core

  App --> Cli
  App --> Core
  App --> Migrate
  App --> Otlp
  App --> Server
```

## Library Usage (Where/How)

```mermaid
flowchart LR
  subgraph HTTP["HTTP + OA3 Hosting"]
    Axum["axum / axum-server"]
    Swagger["swagger (openapi generator runtime)"]
    Tower["tower (service glue)"]
  end

  subgraph DB["Database"]
    Sqlx["sqlx (Postgres, FromRow)"]
    SqlxData["sqlx-data (params + pagination)"]
    Migrations["migrations/*.sql"]
  end

  subgraph Mapping["DTO Mapping"]
    O2O["o2o (derive mapping)"]
  end

  subgraph IDs["IDs"]
    Cuid["cuid (prefix + cuid)"]
  end

  subgraph Cache["Caching"]
    Lru["lru (in-process caches)"]
    Redis["redis:latest (compose; optional integration)"]
  end

  subgraph AWS["AWS Integrations"]
    S3["aws-sdk-s3 (presign PUT)"]
    SNS["aws-sdk-sns (publish + retry worker)"]
    AwsCfg["aws-config (provider chain)"]
  end

  subgraph Obs["Observability"]
    Tracing["tracing + tracing-subscriber"]
  end

  Server["backend-server"] --> Axum
  Server --> Swagger
  Server --> Tower
  Server --> Lru
  Server --> S3
  Server --> SNS
  Server --> AwsCfg
  Server --> Tracing

  Repo["backend-repository"] --> Sqlx
  Repo --> SqlxData
  Repo --> Lru

  Model["backend-model"] --> Sqlx
  Model --> O2O

  Core["backend-core"] --> Cuid

  MigrateCrate["backend-migrate"] --> Sqlx
  MigrateCrate --> Migrations
```

## OpenAPI Regeneration Workflow

All API surface changes start in `openapi/*.json` and flow into generated crates:

```mermaid
flowchart LR
  Spec["openapi/*.json"] --> Gen["docker compose run generate-code"]
  Gen --> Crates["crates/gen_*"]
  Crates --> Server["backend-server controllers"]
  Crates --> Model["backend-model mappings"]
```
