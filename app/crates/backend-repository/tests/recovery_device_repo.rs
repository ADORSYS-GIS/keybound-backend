use anyhow::Result;
use backend_migrate::connect_postgres_and_migrate;
use backend_model::db;
use backend_model::kc::{KcAnyMap, RecoveryBindRequest};
use backend_model::schema::{app_user, device, old_device_policy_idempotency, recovery_idempotency};
use backend_repository::{DeviceRepo, DeviceRepository};
use chrono::Utc;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use diesel_async::pooled_connection::deadpool::Pool;
use gen_oas_server_kc::types::Object;
use serde_json::json;

type DbPool = Pool<diesel_async::AsyncPgConnection>;

fn build_public_jwk() -> KcAnyMap {
    let mut map = KcAnyMap::new();
    map.insert("kty".to_string(), Object(json!("EC")));
    map.insert("crv".to_string(), Object(json!("P-256")));
    map.insert("x".to_string(), Object(json!("x-val")));
    map.insert("y".to_string(), Object(json!("y-val")));
    map
}

fn build_other_public_jwk() -> KcAnyMap {
    let mut map = KcAnyMap::new();
    map.insert("kty".to_string(), Object(json!("EC")));
    map.insert("crv".to_string(), Object(json!("P-256")));
    map.insert("x".to_string(), Object(json!("x-other")));
    map.insert("y".to_string(), Object(json!("y-other")));
    map
}

fn canonical_jwk(map: &KcAnyMap) -> String {
    let mut sorted: std::collections::BTreeMap<String, serde_json::Value> =
        std::collections::BTreeMap::new();
    for (k, v) in map {
        sorted.insert(k.clone(), v.0.clone());
    }
    serde_json::to_string(&sorted).expect("jwk serialization")
}

async fn seed_user(pool: &DbPool, user_id: &str, username: &str) -> Result<()> {
    let mut conn = pool.get().await?;
    diesel::insert_into(app_user::table)
        .values((
            app_user::user_id.eq(user_id),
            app_user::realm.eq("test"),
            app_user::username.eq(username),
            app_user::disabled.eq(false),
            app_user::email_verified.eq(true),
            app_user::created_at.eq(Utc::now()),
            app_user::updated_at.eq(Utc::now()),
        ))
        .execute(&mut conn)
        .await?;
    Ok(())
}

async fn insert_device_row(
    pool: &DbPool,
    user_id: &str,
    device_id: &str,
    jkt: &str,
    public_jwk: &KcAnyMap,
    status: &str,
) -> Result<String> {
    let mut conn = pool.get().await?;
    let public_jwk_str = canonical_jwk(public_jwk);
    let record_id = backend_model::kc::device_record_id(device_id, &public_jwk_str);
    let row = db::DeviceRow {
        device_id: device_id.to_string(),
        user_id: user_id.to_string(),
        jkt: jkt.to_string(),
        public_jwk: public_jwk_str,
        device_record_id: record_id.clone(),
        status: status.to_string(),
        label: None,
        created_at: Utc::now(),
        last_seen_at: Some(Utc::now()),
    };
    diesel::insert_into(device::table)
        .values(&row)
        .execute(&mut conn)
        .await?;
    Ok(record_id)
}

async fn cleanup(
    pool: &DbPool,
    idempotency_keys: &[&str],
    device_ids: &[&str],
    user_id: &str,
) -> Result<()> {
    let mut conn = pool.get().await?;
    for k in idempotency_keys {
        diesel::delete(
            recovery_idempotency::table.filter(recovery_idempotency::idempotency_key.eq(k)),
        )
        .execute(&mut conn)
        .await?;
        diesel::delete(
            old_device_policy_idempotency::table
                .filter(old_device_policy_idempotency::idempotency_key.eq(k)),
        )
        .execute(&mut conn)
        .await?;
    }
    for d in device_ids {
        diesel::delete(device::table.filter(device::device_id.eq(d)))
            .execute(&mut conn)
            .await?;
    }
    diesel::delete(app_user::table.filter(app_user::user_id.eq(user_id)))
        .execute(&mut conn)
        .await?;
    Ok(())
}

fn expect_conflict<T: std::fmt::Debug>(
    res: &std::result::Result<T, backend_core::Error>,
    expected_key: &str,
) -> Result<()> {
    match res {
        Err(backend_core::Error::Http {
            status_code,
            error_key,
            ..
        }) => {
            assert_eq!(*status_code, 409, "expected HTTP 409 conflict");
            assert_eq!(
                *error_key, expected_key,
                "expected error key {expected_key}"
            );
            Ok(())
        }
        other => Err(anyhow::anyhow!(
            "expected HTTP 409 {expected_key}, got {:?}",
            other
        )),
    }
}

#[tokio::test]
async fn recovery_device_bind_flow() -> Result<()> {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            eprintln!("Skipping recovery device bind test because DATABASE_URL is not set");
            return Ok(());
        }
    };

    let pool = connect_postgres_and_migrate(&database_url).await?;
    let repo = DeviceRepository::new(pool.clone());

    let user_id = backend_id::user_id()?.to_string();
    let device_id = backend_id::device_id()?;
    let jkt = "test-recovery-jkt".to_string();
    let idempotency_key = "550e8400-e29b-41d4-a716-446655440000".to_string();
    let recovery_case_id = "rc_test_123".to_string();
    let request_hash = "hash_123456789".to_string();
    let binding_op_id = "op_test_123".to_string();

    // 1. Seed user in app_user
    {
        let mut conn = pool.get().await?;
        diesel::insert_into(app_user::table)
            .values((
                app_user::user_id.eq(&user_id),
                app_user::realm.eq("test"),
                app_user::username.eq("test-recovery-user"),
                app_user::disabled.eq(false),
                app_user::email_verified.eq(true),
                app_user::created_at.eq(Utc::now()),
                app_user::updated_at.eq(Utc::now()),
            ))
            .execute(&mut conn)
            .await?;
    }

    let req = RecoveryBindRequest {
        realm: "test".to_string(),
        target_user_id: user_id.clone(),
        approval_revision: 5,
        device_id: device_id.clone(),
        jkt: jkt.clone(),
        public_jwk: build_public_jwk(),
        binding_operation_id: binding_op_id.clone(),
    };

    // 2. Perform recovery bind
    let record_id = repo
        .bind_recovery_device(&idempotency_key, &recovery_case_id, &request_hash, &req)
        .await?;

    assert!(!record_id.is_empty());

    // 3. Verify idempotency record query
    let stored_idempotency = repo
        .find_recovery_idempotency(&idempotency_key)
        .await?
        .expect("idempotency record should exist");

    assert_eq!(stored_idempotency.request_hash, request_hash);
    assert_eq!(stored_idempotency.bound_user_id, user_id);
    assert_eq!(stored_idempotency.device_id, device_id);
    assert_eq!(stored_idempotency.binding_operation_id, binding_op_id);
    assert_eq!(stored_idempotency.device_record_id, record_id);

    // 4. Cleanup test data
    {
        let mut conn = pool.get().await?;
        diesel::delete(
            recovery_idempotency::table
                .filter(recovery_idempotency::idempotency_key.eq(&idempotency_key)),
        )
        .execute(&mut conn)
        .await?;
        diesel::delete(device::table.filter(device::device_id.eq(&device_id)))
            .execute(&mut conn)
            .await?;
        diesel::delete(app_user::table.filter(app_user::user_id.eq(&user_id)))
            .execute(&mut conn)
            .await?;
    }

    Ok(())
}

#[tokio::test]
async fn concurrent_same_key_same_payload_is_idempotent() -> Result<()> {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            eprintln!("Skipping recovery device bind test because DATABASE_URL is not set");
            return Ok(());
        }
    };

    let pool = connect_postgres_and_migrate(&database_url).await?;
    let repo = DeviceRepository::new(pool.clone());

    let user_id = backend_id::user_id()?.to_string();
    let device_id = backend_id::device_id()?;
    let jkt = "test-concurrent-jkt-same".to_string();
    let idempotency_key = "550e8400-e29b-41d4-a716-446655440001".to_string();
    let recovery_case_id = "rc_concurrent_same_001".to_string();
    let request_hash = "hash_concurrent_same_001".to_string();
    let binding_op_id = "op_concurrent_same_001".to_string();

    // 1. Seed user in app_user
    {
        let mut conn = pool.get().await?;
        diesel::insert_into(app_user::table)
            .values((
                app_user::user_id.eq(&user_id),
                app_user::realm.eq("test"),
                app_user::username.eq("test-concurrent-same-user"),
                app_user::disabled.eq(false),
                app_user::email_verified.eq(true),
                app_user::created_at.eq(Utc::now()),
                app_user::updated_at.eq(Utc::now()),
            ))
            .execute(&mut conn)
            .await?;
    }

    let req = RecoveryBindRequest {
        realm: "test".to_string(),
        target_user_id: user_id.clone(),
        approval_revision: 5,
        device_id: device_id.clone(),
        jkt: jkt.clone(),
        public_jwk: build_public_jwk(),
        binding_operation_id: binding_op_id.clone(),
    };

    // 2. Fire two identical recovery binds concurrently
    let (res_a, res_b) = tokio::join!(
        repo.bind_recovery_device(&idempotency_key, &recovery_case_id, &request_hash, &req),
        repo.bind_recovery_device(&idempotency_key, &recovery_case_id, &request_hash, &req),
    );

    let record_id_a = res_a?;
    let record_id_b = res_b?;

    assert!(!record_id_a.is_empty());
    assert_eq!(record_id_a, record_id_b);

    // 3. Verify a single idempotency record exists with the winning device_record_id
    let stored_idempotency = repo
        .find_recovery_idempotency(&idempotency_key)
        .await?
        .expect("idempotency record should exist");
    assert_eq!(stored_idempotency.device_record_id, record_id_a);

    // 4. Cleanup test data
    {
        let mut conn = pool.get().await?;
        diesel::delete(
            recovery_idempotency::table
                .filter(recovery_idempotency::idempotency_key.eq(&idempotency_key)),
        )
        .execute(&mut conn)
        .await?;
        diesel::delete(device::table.filter(device::device_id.eq(&device_id)))
            .execute(&mut conn)
            .await?;
        diesel::delete(app_user::table.filter(app_user::user_id.eq(&user_id)))
            .execute(&mut conn)
            .await?;
    }

    Ok(())
}

#[tokio::test]
async fn concurrent_same_key_different_payload_conflicts() -> Result<()> {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            eprintln!("Skipping recovery device bind test because DATABASE_URL is not set");
            return Ok(());
        }
    };

    let pool = connect_postgres_and_migrate(&database_url).await?;
    let repo = DeviceRepository::new(pool.clone());

    let user_id = backend_id::user_id()?.to_string();
    let device_id = backend_id::device_id()?;
    let jkt = "test-concurrent-jkt-diff".to_string();
    let idempotency_key = "550e8400-e29b-41d4-a716-446655440002".to_string();
    let recovery_case_id = "rc_concurrent_diff_002".to_string();
    let request_hash_a = "hash_concurrent_diff_a".to_string();
    let request_hash_b = "hash_concurrent_diff_b".to_string();
    let binding_op_id = "op_concurrent_diff_002".to_string();

    // 1. Seed user in app_user
    {
        let mut conn = pool.get().await?;
        diesel::insert_into(app_user::table)
            .values((
                app_user::user_id.eq(&user_id),
                app_user::realm.eq("test"),
                app_user::username.eq("test-concurrent-diff-user"),
                app_user::disabled.eq(false),
                app_user::email_verified.eq(true),
                app_user::created_at.eq(Utc::now()),
                app_user::updated_at.eq(Utc::now()),
            ))
            .execute(&mut conn)
            .await?;
    }

    let req = RecoveryBindRequest {
        realm: "test".to_string(),
        target_user_id: user_id.clone(),
        approval_revision: 5,
        device_id: device_id.clone(),
        jkt: jkt.clone(),
        public_jwk: build_public_jwk(),
        binding_operation_id: binding_op_id.clone(),
    };

    // 2. Fire two recovery binds concurrently with the same key but different payloads
    let (res_a, res_b) = tokio::join!(
        repo.bind_recovery_device(&idempotency_key, &recovery_case_id, &request_hash_a, &req),
        repo.bind_recovery_device(&idempotency_key, &recovery_case_id, &request_hash_b, &req),
    );

    let (ok_res, err_res) = match (&res_a, &res_b) {
        (Ok(id), Err(err)) | (Err(err), Ok(id)) => (id, err),
        _ => {
            return Err(anyhow::anyhow!(
                "expected exactly one success and one conflict, got {:?} and {:?}",
                res_a,
                res_b
            ));
        }
    };

    assert!(!ok_res.is_empty());
    match err_res {
        backend_core::Error::Http {
            status_code,
            error_key,
            ..
        } => {
            assert_eq!(*status_code, 409);
            assert_eq!(*error_key, "IDEMPOTENCY_CONFLICT");
        }
        other => {
            return Err(anyhow::anyhow!(
                "expected HTTP 409 IDEMPOTENCY_CONFLICT, got {:?}",
                other
            ));
        }
    }

    // 3. Cleanup test data
    {
        let mut conn = pool.get().await?;
        diesel::delete(
            recovery_idempotency::table
                .filter(recovery_idempotency::idempotency_key.eq(&idempotency_key)),
        )
        .execute(&mut conn)
        .await?;
        diesel::delete(device::table.filter(device::device_id.eq(&device_id)))
            .execute(&mut conn)
            .await?;
        diesel::delete(app_user::table.filter(app_user::user_id.eq(&user_id)))
            .execute(&mut conn)
            .await?;
    }

    Ok(())
}

#[tokio::test]
async fn same_user_same_device_id_different_jkt_is_rejected() -> Result<()> {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            eprintln!("Skipping recovery device bind test because DATABASE_URL is not set");
            return Ok(());
        }
    };

    let pool = connect_postgres_and_migrate(&database_url).await?;
    let repo = DeviceRepository::new(pool.clone());

    let user_id = backend_id::user_id()?.to_string();
    let device_id = backend_id::device_id()?.to_string();
    let idem_key = "550e8400-e29b-41d4-a716-446655440010";
    let seed_jkt = "jkt-old-J1".to_string();

    seed_user(&pool, &user_id, "recovery-collision-same-device").await?;
    // Same user already owns device D bound to old key J1 / public JWK A.
    let existing_record_id = insert_device_row(
        &pool,
        &user_id,
        &device_id,
        &seed_jkt,
        &build_public_jwk(),
        "ACTIVE",
    )
    .await?;

    // Approved recovery requests the exact same device D but a NEW key J2 / public JWK B.
    let req = RecoveryBindRequest {
        realm: "test".to_string(),
        target_user_id: user_id.clone(),
        approval_revision: 5,
        device_id: device_id.clone(),
        jkt: "jkt-new-J2".to_string(),
        public_jwk: build_other_public_jwk(),
        binding_operation_id: "op_collision_same_device_1".to_string(),
    };
    let res = repo
        .bind_recovery_device(idem_key, "rc_col_same_dev_1", "hash_col_same_dev_1", &req)
        .await;
    expect_conflict(&res, "DEVICE_CREDENTIAL_MISMATCH")?;

    // No false success: no idempotency row is recorded and the stored device is unchanged.
    assert!(repo.find_recovery_idempotency(idem_key).await?.is_none());
    let mut conn = pool.get().await?;
    let stored = device::table
        .filter(device::device_id.eq(&device_id))
        .first::<db::DeviceRow>(&mut conn)
        .await
        .optional()?
        .expect("device row should still exist");
    assert_eq!(stored.jkt, seed_jkt);
    assert_eq!(stored.status, "ACTIVE");
    assert_eq!(stored.device_record_id, existing_record_id);

    cleanup(&pool, &[idem_key], &[&device_id], &user_id).await?;
    Ok(())
}

#[tokio::test]
async fn same_user_same_jkt_different_device_id_is_rejected() -> Result<()> {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            eprintln!("Skipping recovery device bind test because DATABASE_URL is not set");
            return Ok(());
        }
    };

    let pool = connect_postgres_and_migrate(&database_url).await?;
    let repo = DeviceRepository::new(pool.clone());

    let user_id = backend_id::user_id()?.to_string();
    let device_1 = backend_id::device_id()?.to_string();
    let device_2 = backend_id::device_id()?.to_string();
    let idem_key = "550e8400-e29b-41d4-a716-446655440011";
    let shared_jkt = "jkt-shared-J".to_string();

    seed_user(&pool, &user_id, "recovery-collision-same-jkt").await?;
    // Same user already owns key J bound to device D1 / public JWK A.
    insert_device_row(
        &pool,
        &user_id,
        &device_1,
        &shared_jkt,
        &build_public_jwk(),
        "ACTIVE",
    )
    .await?;

    // Approved recovery requests a different device D2 reusing the same JKT J with a new public JWK B.
    let req = RecoveryBindRequest {
        realm: "test".to_string(),
        target_user_id: user_id.clone(),
        approval_revision: 5,
        device_id: device_2.clone(),
        jkt: shared_jkt.clone(),
        public_jwk: build_other_public_jwk(),
        binding_operation_id: "op_collision_same_jkt_2".to_string(),
    };
    let res = repo
        .bind_recovery_device(idem_key, "rc_col_same_jkt_2", "hash_col_same_jkt_2", &req)
        .await;
    expect_conflict(&res, "DEVICE_CREDENTIAL_MISMATCH")?;

    assert!(repo.find_recovery_idempotency(idem_key).await?.is_none());

    cleanup(&pool, &[idem_key], &[&device_1, &device_2], &user_id).await?;
    Ok(())
}

#[tokio::test]
async fn exact_existing_credential_in_revoked_state_is_rejected() -> Result<()> {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            eprintln!("Skipping recovery device bind test because DATABASE_URL is not set");
            return Ok(());
        }
    };

    let pool = connect_postgres_and_migrate(&database_url).await?;
    let repo = DeviceRepository::new(pool.clone());

    let user_id = backend_id::user_id()?.to_string();
    let device_id = backend_id::device_id()?.to_string();
    let idem_key = "550e8400-e29b-41d4-a716-446655440012";
    let jkt = "jkt-exact-revoked".to_string();

    seed_user(&pool, &user_id, "recovery-revoked-exact").await?;
    // Exact approved credential already exists but is REVOKED.
    insert_device_row(
        &pool,
        &user_id,
        &device_id,
        &jkt,
        &build_public_jwk(),
        "REVOKED",
    )
    .await?;

    let req = RecoveryBindRequest {
        realm: "test".to_string(),
        target_user_id: user_id.clone(),
        approval_revision: 5,
        device_id: device_id.clone(),
        jkt: jkt.clone(),
        public_jwk: build_public_jwk(),
        binding_operation_id: "op_revoked_exact_3".to_string(),
    };
    let res = repo
        .bind_recovery_device(idem_key, "rc_revoked_exact_3", "hash_revoked_exact_3", &req)
        .await;
    expect_conflict(&res, "DEVICE_NOT_ACTIVE")?;

    assert!(repo.find_recovery_idempotency(idem_key).await?.is_none());

    cleanup(&pool, &[idem_key], &[&device_id], &user_id).await?;
    Ok(())
}

#[tokio::test]
async fn exact_existing_active_credential_is_idempotent() -> Result<()> {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            eprintln!("Skipping recovery device bind test because DATABASE_URL is not set");
            return Ok(());
        }
    };

    let pool = connect_postgres_and_migrate(&database_url).await?;
    let repo = DeviceRepository::new(pool.clone());

    let user_id = backend_id::user_id()?.to_string();
    let device_id = backend_id::device_id()?.to_string();
    let idem_key = "550e8400-e29b-41d4-a716-446655440013";
    let jkt = "jkt-exact-active".to_string();

    seed_user(&pool, &user_id, "recovery-active-exact").await?;
    // Exact approved credential already exists and is ACTIVE (legitimate idempotent case).
    let existing_record_id = insert_device_row(
        &pool,
        &user_id,
        &device_id,
        &jkt,
        &build_public_jwk(),
        "ACTIVE",
    )
    .await?;

    let req = RecoveryBindRequest {
        realm: "test".to_string(),
        target_user_id: user_id.clone(),
        approval_revision: 5,
        device_id: device_id.clone(),
        jkt: jkt.clone(),
        public_jwk: build_public_jwk(),
        binding_operation_id: "op_active_exact_4".to_string(),
    };
    let bound = repo
        .bind_recovery_device(idem_key, "rc_active_exact_4", "hash_active_exact_4", &req)
        .await?;

    // Idempotent replay of the exact active credential returns the persisted record id.
    assert_eq!(bound, existing_record_id);

    let idem = repo
        .find_recovery_idempotency(idem_key)
        .await?
        .expect("idempotency row should be written for the exact active credential");
    assert_eq!(idem.device_record_id, existing_record_id);

    cleanup(&pool, &[idem_key], &[&device_id], &user_id).await?;
    Ok(())
}

#[tokio::test]
async fn revoke_all_previous_revokes_old_devices_but_keeps_new() -> Result<()> {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            eprintln!("Skipping old-device policy test because DATABASE_URL is not set");
            return Ok(());
        }
    };

    let pool = connect_postgres_and_migrate(&database_url).await?;
    let repo = DeviceRepository::new(pool.clone());

    let user_id = backend_id::user_id()?.to_string();
    let old1 = backend_id::device_id()?.to_string();
    let old2 = backend_id::device_id()?.to_string();
    let new_dev = backend_id::device_id()?.to_string();
    let idem_key = "550e8400-e29b-41d4-a716-446655440021";

    seed_user(&pool, &user_id, "recovery-revoke-all").await?;
    let old1_record = insert_device_row(&pool, &user_id, &old1, "jkt-old1", &build_public_jwk(), "ACTIVE").await?;
    let old2_record = insert_device_row(&pool, &user_id, &old2, "jkt-old2", &build_other_public_jwk(), "ACTIVE").await?;
    let new_record = insert_device_row(&pool, &user_id, &new_dev, "jkt-new", &build_public_jwk(), "ACTIVE").await?;

    let outcome = repo
        .apply_old_device_policy(
            idem_key,
            "rc_revoke_all",
            "hash_revoke_all",
            &user_id,
            "REVOKE_ALL_PREVIOUS",
            &[new_dev.clone()],
        )
        .await?;

    assert!(!outcome.already_applied);
    assert!(outcome.affected_device_ids.contains(&old1_record));
    assert!(outcome.affected_device_ids.contains(&old2_record));
    assert!(!outcome.affected_device_ids.contains(&new_record));

    // Old devices are REVOKED, the new device stays ACTIVE.
    let mut conn = pool.get().await?;
    let statuses: Vec<String> = device::table
        .filter(device::user_id.eq(&user_id))
        .select(device::status)
        .load::<String>(&mut conn)
        .await?;
    assert_eq!(statuses.iter().filter(|s| s.as_str() == "REVOKED").count(), 2);
    assert_eq!(statuses.iter().filter(|s| s.as_str() == "ACTIVE").count(), 1);

    cleanup(&pool, &[idem_key], &[&old1, &old2, &new_dev], &user_id).await?;
    Ok(())
}

#[tokio::test]
async fn quarantine_all_previous_sets_quarantined_status() -> Result<()> {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            eprintln!("Skipping old-device policy test because DATABASE_URL is not set");
            return Ok(());
        }
    };

    let pool = connect_postgres_and_migrate(&database_url).await?;
    let repo = DeviceRepository::new(pool.clone());

    let user_id = backend_id::user_id()?.to_string();
    let old1 = backend_id::device_id()?.to_string();
    let new_dev = backend_id::device_id()?.to_string();
    let idem_key = "550e8400-e29b-41d4-a716-446655440022";

    seed_user(&pool, &user_id, "recovery-quarantine").await?;
    let old1_record = insert_device_row(&pool, &user_id, &old1, "jkt-q-old1", &build_public_jwk(), "ACTIVE").await?;
    insert_device_row(&pool, &user_id, &new_dev, "jkt-q-new", &build_other_public_jwk(), "ACTIVE").await?;

    let outcome = repo
        .apply_old_device_policy(
            idem_key,
            "rc_quarantine",
            "hash_quarantine",
            &user_id,
            "QUARANTINE_ALL_PREVIOUS",
            &[new_dev.clone()],
        )
        .await?;

    assert!(outcome.affected_device_ids.contains(&old1_record));

    let mut conn = pool.get().await?;
    let old_status: String = device::table
        .filter(device::device_id.eq(&old1))
        .select(device::status)
        .first::<String>(&mut conn)
        .await?;
    assert_eq!(old_status, "QUARANTINED");

    cleanup(&pool, &[idem_key], &[&old1, &new_dev], &user_id).await?;
    Ok(())
}

#[tokio::test]
async fn old_device_policy_is_idempotent_on_retry() -> Result<()> {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            eprintln!("Skipping old-device policy test because DATABASE_URL is not set");
            return Ok(());
        }
    };

    let pool = connect_postgres_and_migrate(&database_url).await?;
    let repo = DeviceRepository::new(pool.clone());

    let user_id = backend_id::user_id()?.to_string();
    let old1 = backend_id::device_id()?.to_string();
    let new_dev = backend_id::device_id()?.to_string();
    let idem_key = "550e8400-e29b-41d4-a716-446655440023";

    seed_user(&pool, &user_id, "recovery-idem-policy").await?;
    let old1_record = insert_device_row(&pool, &user_id, &old1, "jkt-i-old1", &build_public_jwk(), "ACTIVE").await?;
    insert_device_row(&pool, &user_id, &new_dev, "jkt-i-new", &build_other_public_jwk(), "ACTIVE").await?;

    let first = repo
        .apply_old_device_policy(
            idem_key,
            "rc_idem_policy",
            "hash_idem_policy",
            &user_id,
            "REVOKE_ALL_PREVIOUS",
            &[new_dev.clone()],
        )
        .await?;
    assert!(!first.already_applied);

    // Retry with the same idempotency key and identical payload returns the same result.
    let retry = repo
        .apply_old_device_policy(
            idem_key,
            "rc_idem_policy",
            "hash_idem_policy",
            &user_id,
            "REVOKE_ALL_PREVIOUS",
            &[new_dev.clone()],
        )
        .await?;
    assert!(retry.already_applied);
    assert_eq!(retry.affected_device_ids, first.affected_device_ids);
    assert!(retry.affected_device_ids.contains(&old1_record));

    cleanup(&pool, &[idem_key], &[&old1, &new_dev], &user_id).await?;
    Ok(())
}

#[tokio::test]
async fn old_device_policy_rejects_modified_payload_on_idempotency_reuse() -> Result<()> {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            eprintln!("Skipping old-device policy test because DATABASE_URL is not set");
            return Ok(());
        }
    };

    let pool = connect_postgres_and_migrate(&database_url).await?;
    let repo = DeviceRepository::new(pool.clone());

    let user_id = backend_id::user_id()?.to_string();
    let old1 = backend_id::device_id()?.to_string();
    let new_dev = backend_id::device_id()?.to_string();
    let idem_key = "550e8400-e29b-41d4-a716-446655440024";

    seed_user(&pool, &user_id, "recovery-conflict-policy").await?;
    insert_device_row(&pool, &user_id, &old1, "jkt-c-old1", &build_public_jwk(), "ACTIVE").await?;
    insert_device_row(&pool, &user_id, &new_dev, "jkt-c-new", &build_other_public_jwk(), "ACTIVE").await?;

    repo.apply_old_device_policy(
        idem_key,
        "rc_conflict_policy",
        "hash_conflict_policy",
        &user_id,
        "REVOKE_ALL_PREVIOUS",
        &[new_dev.clone()],
    )
    .await?;

    // Reusing the same idempotency key with a different policy must conflict.
    let res = repo
        .apply_old_device_policy(
            idem_key,
            "rc_conflict_policy",
            "hash_conflict_policy",
            &user_id,
            "QUARANTINE_ALL_PREVIOUS",
            &[new_dev.clone()],
        )
        .await;
    expect_conflict(&res, "IDEMPOTENCY_CONFLICT")?;

    cleanup(&pool, &[idem_key], &[&old1, &new_dev], &user_id).await?;
    Ok(())
}

#[tokio::test]
async fn concurrent_old_device_policy_applies_one_and_conflicts_the_other() -> Result<()> {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            eprintln!("Skipping old-device policy test because DATABASE_URL is not set");
            return Ok(());
        }
    };

    let pool = connect_postgres_and_migrate(&database_url).await?;
    let repo = DeviceRepository::new(pool.clone());

    let user_id = backend_id::user_id()?.to_string();
    let old1 = backend_id::device_id()?.to_string();
    let new_dev = backend_id::device_id()?.to_string();
    let idem_a = "550e8400-e29b-41d4-a716-446655440025";
    let idem_b = "550e8400-e29b-41d4-a716-446655440026";

    seed_user(&pool, &user_id, "recovery-concurrent-policy").await?;
    insert_device_row(&pool, &user_id, &old1, "jkt-cc-old1", &build_public_jwk(), "ACTIVE").await?;
    insert_device_row(&pool, &user_id, &new_dev, "jkt-cc-new", &build_other_public_jwk(), "ACTIVE").await?;

    let expect_new = vec![new_dev.clone()];

    let res_a = repo.apply_old_device_policy(
        idem_a,
        "rc_concurrent_policy",
        "hash_concurrent_a",
        &user_id,
        "REVOKE_ALL_PREVIOUS",
        &expect_new,
    );
    let res_b = repo.apply_old_device_policy(
        idem_b,
        "rc_concurrent_policy",
        "hash_concurrent_b",
        &user_id,
        "QUARANTINE_ALL_PREVIOUS",
        &expect_new,
    );

    let (ra, rb) = tokio::join!(res_a, res_b);

    // Exactly one finalization must win; the losing transaction must conflict on
    // the case-level row rather than return its locally-computed success.
    let mut ok_count = 0;
    let mut conflict_count = 0;
    for res in [ra, rb] {
        match res {
            Ok(outcome) => {
                assert!(!outcome.already_applied);
                ok_count += 1;
            }
            Err(backend_core::Error::Http {
                status_code: 409,
                error_key,
                ..
            }) => {
                assert_eq!(error_key, "POLICY_ALREADY_APPLIED");
                conflict_count += 1;
            }
            Err(other) => return Err(anyhow::anyhow!("unexpected error: {:?}", other)),
        }
    }
    assert_eq!(ok_count, 1, "exactly one concurrent finalization must win");
    assert_eq!(conflict_count, 1, "the losing finalization must conflict");

    // Registry state must be consistent with the persisted (winning) idempotency
    // record: the newly bound device stays ACTIVE, superseded devices are not.
    let mut conn = pool.get().await?;
    let new_status: Option<String> = device::table
        .filter(device::device_id.eq(&new_dev))
        .select(device::status)
        .first::<String>(&mut conn)
        .await
        .ok();
    assert_eq!(new_status.as_deref(), Some("ACTIVE"));

    let old_status: String = device::table
        .filter(device::device_id.eq(&old1))
        .select(device::status)
        .first::<String>(&mut conn)
        .await?;
    assert!(
        old_status == "REVOKED" || old_status == "QUARANTINED",
        "superseded device must be REVOKED or QUARANTINED, got {old_status}"
    );

    cleanup(&pool, &[idem_a, idem_b], &[&old1, &new_dev], &user_id).await?;
    Ok(())
}
