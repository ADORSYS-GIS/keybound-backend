use anyhow::Result;
use backend_migrate::connect_postgres_and_migrate;
use backend_model::db;
use backend_model::schema::recovery_case;
use backend_repository::{
    RecoveryCaseCreateInput, RecoveryCaseFilter, RecoveryCaseRepo, RecoveryCaseRepository,
    RecoveryCaseUpdate,
};
use chrono::Utc;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use diesel_async::pooled_connection::deadpool::Pool;
use serde_json::json;

type DbPool = Pool<diesel_async::AsyncPgConnection>;

fn create_input(id: &str, phone_hash: &str, masked: &str) -> RecoveryCaseCreateInput {
    RecoveryCaseCreateInput {
        id: id.to_string(),
        human_id: format!("rc.{id}"),
        session_id: None,
        device_id: None,
        jkt: None,
        device_public_jwk: None,
        requested_phone_hash: phone_hash.to_string(),
        requested_phone_masked: masked.to_string(),
        reason: Some("lost-device".to_string()),
        status: "CREATED".to_string(),
        phone_relation: None,
        matched_user_id: None,
        expires_at: Some(Utc::now() + chrono::Duration::minutes(30)),
    }
}

async fn cleanup(pool: &DbPool, ids: &[&str]) -> Result<()> {
    let mut conn = pool.get().await?;
    diesel::delete(recovery_case::table.filter(recovery_case::id.eq_any(ids)))
        .execute(&mut conn)
        .await?;
    Ok(())
}

#[tokio::test]
async fn recovery_case_crud_and_versioned_update() -> Result<()> {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            eprintln!("Skipping recovery case repo test because DATABASE_URL is not set");
            return Ok(());
        }
    };

    let pool = connect_postgres_and_migrate(&database_url).await?;
    let repo = RecoveryCaseRepository::new(pool.clone());

    let id = "rc_test_001";
    let phone_hash = "abc123hash";
    let masked = "+237****0000";

    let created = repo
        .create_case(create_input(id, phone_hash, masked))
        .await?;
    assert_eq!(created.status, "CREATED");
    assert_eq!(created.version, 1);
    assert_eq!(created.requested_phone_masked, masked);

    let fetched = repo.get_case_by_id(id).await?.expect("case exists");
    assert_eq!(fetched.id, id);
    assert_eq!(fetched.requested_phone_hash, phone_hash);

    // Advance to EVIDENCE_REQUIRED + set OTP hash via versioned update.
    let mut update = RecoveryCaseUpdate::default();
    update.status = Some("EVIDENCE_REQUIRED".to_string());
    update.otp_hash = Some(Some("hashed_otp".to_string()));
    update.matched_user_id = Some(Some("usr_123".to_string()));

    let updated = repo.update_case(id, 1, &update).await?;
    assert_eq!(updated.status, "EVIDENCE_REQUIRED");
    assert_eq!(updated.otp_hash.as_deref(), Some("hashed_otp"));
    assert_eq!(updated.matched_user_id.as_deref(), Some("usr_123"));
    assert_eq!(updated.version, 2);

    // Optimistic concurrency: stale version conflicts.
    let stale = repo.update_case(id, 1, &update).await;
    assert!(stale.is_err(), "stale version must conflict");
    if let Err(backend_core::Error::Http {
        error_key,
        status_code,
        ..
    }) = stale
    {
        assert_eq!(status_code, 409);
        assert_eq!(error_key, "RECOVERY_CASE_VERSION_CONFLICT");
    } else {
        panic!("expected HTTP 409 conflict");
    }

    // Re-fetch to confirm the last successful update won.
    let refreshed = repo.get_case_by_id(id).await?.expect("case exists");
    assert_eq!(refreshed.version, 2);

    cleanup(&pool, &[id]).await?;
    Ok(())
}

#[tokio::test]
async fn recovery_case_lookup_by_phone_hash_and_list_filter() -> Result<()> {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            eprintln!("Skipping recovery case repo test because DATABASE_URL is not set");
            return Ok(());
        }
    };

    let pool = connect_postgres_and_migrate(&database_url).await?;
    let repo = RecoveryCaseRepository::new(pool.clone());

    let id_a = "rc_test_lookup_a";
    let id_b = "rc_test_lookup_b";
    let hash = "shared_phone_hash_xyz";

    let mut input = create_input(id_a, hash, "+237****0001");
    input.matched_user_id = Some("usr_a".to_string());
    repo.create_case(input).await?;

    let mut input_b = create_input(id_b, hash, "+237****0002");
    input_b.matched_user_id = Some("usr_b".to_string());
    repo.create_case(input_b).await?;

    // Lookup by phone hash returns the most recent case.
    let by_hash = repo
        .get_case_by_phone_hash(hash)
        .await?
        .expect("case by hash");
    assert!(by_hash.id == id_a || by_hash.id == id_b);

    // Lookup by id.
    let by_id = repo.get_case_by_id(id_a).await?.expect("case by id");
    assert_eq!(by_id.id, id_a);

    // List filtered by status.
    let (rows, total) = repo
        .list_cases(RecoveryCaseFilter {
            status: Some("CREATED".to_string()),
            matched_user_id: None,
            page: 1,
            limit: 10,
        })
        .await?;
    assert!(total >= 2);
    assert_eq!(rows.len(), total.min(10) as usize);

    // List filtered by matched user.
    let (rows_a, total_a) = repo
        .list_cases(RecoveryCaseFilter {
            status: None,
            matched_user_id: Some("usr_a".to_string()),
            page: 1,
            limit: 10,
        })
        .await?;
    assert_eq!(total_a, 1);
    assert_eq!(rows_a[0].id, id_a);

    cleanup(&pool, &[id_a, id_b]).await?;
    Ok(())
}
