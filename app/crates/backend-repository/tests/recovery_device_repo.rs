use anyhow::Result;
use backend_migrate::connect_postgres_and_migrate;
use backend_model::kc::{KcAnyMap, RecoveryBindRequest};
use backend_model::schema::{app_user, device, recovery_idempotency};
use backend_repository::{DeviceRepo, DeviceRepository};
use chrono::Utc;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use gen_oas_server_kc::types::Object;
use serde_json::json;

fn build_public_jwk() -> KcAnyMap {
    let mut map = KcAnyMap::new();
    map.insert("kty".to_string(), Object(json!("EC")));
    map.insert("crv".to_string(), Object(json!("P-256")));
    map.insert("x".to_string(), Object(json!("x-val")));
    map.insert("y".to_string(), Object(json!("y-val")));
    map
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
