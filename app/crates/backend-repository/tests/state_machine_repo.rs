use anyhow::Result;
use backend_migrate::connect_postgres_and_migrate;
use backend_model::schema::{app_user, sm_instance};
use backend_repository::{SmInstanceCreateInput, SmInstanceFilter, SmStepAttemptCreateInput, StateMachineRepo, StateMachineRepository};
use chrono::Utc;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use serde_json::json;

#[tokio::test]
async fn sm_instance_idempotency_and_active_uniqueness() -> Result<()> {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            eprintln!("Skipping backend-repository state machine test because DATABASE_URL is not set");
            return Ok(());
        }
    };

    let pool = connect_postgres_and_migrate(&database_url).await?;
    let repo = StateMachineRepository::new(pool.clone());

    let user_id = backend_id::user_id()?;

    {
        let mut conn = pool.get().await?;
        let phone_number = "+237690000001";
        diesel::insert_into(app_user::table)
            .values((
                app_user::user_id.eq(&user_id),
                app_user::realm.eq("test"),
                app_user::username.eq("test-user"),
                app_user::phone_number.eq(Some(phone_number)),
                app_user::disabled.eq(false),
                app_user::email_verified.eq(true),
                app_user::created_at.eq(Utc::now()),
                app_user::updated_at.eq(Utc::now()),
            ))
            .execute(&mut conn)
            .await?;
    }

    let idempotency_key = format!("KYC_PHONE_OTP:{user_id}");
    let instance_id = backend_id::sm_instance_id()?;
    let created = repo
        .create_instance(SmInstanceCreateInput {
            id: instance_id.clone(),
            kind: "KYC_PHONE_OTP".to_owned(),
            user_id: Some(user_id.clone()),
            idempotency_key: idempotency_key.clone(),
            status: "ACTIVE".to_owned(),
            context: json!({}),
        })
        .await?;
    assert_eq!(created.id, instance_id);

    let fetched = repo
        .get_instance_by_idempotency_key(&idempotency_key)
        .await?
        .expect("instance should be found by idempotency key");
    assert_eq!(fetched.id, instance_id);

    // Active uniqueness: another ACTIVE instance for the same (user_id, kind) should fail.
    let conflict = repo
        .create_instance(SmInstanceCreateInput {
            id: backend_id::sm_instance_id()?,
            kind: "KYC_PHONE_OTP".to_owned(),
            user_id: Some(user_id.clone()),
            idempotency_key: format!("KYC_PHONE_OTP:{user_id}:other"),
            status: "ACTIVE".to_owned(),
            context: json!({}),
        })
        .await;
    assert!(conflict.is_err());

    // But a COMPLETED instance is allowed.
    let completed = repo
        .create_instance(SmInstanceCreateInput {
            id: backend_id::sm_instance_id()?,
            kind: "KYC_PHONE_OTP".to_owned(),
            user_id: Some(user_id.clone()),
            idempotency_key: format!("KYC_PHONE_OTP:{user_id}:completed"),
            status: "COMPLETED".to_owned(),
            context: json!({}),
        })
        .await?;
    assert_eq!(completed.status, "COMPLETED");

    // Listing works.
    let (items, total) = repo
        .list_instances(SmInstanceFilter {
            kind: Some("KYC_PHONE_OTP".to_owned()),
            status: None,
            user_id: Some(user_id.clone()),
            phone_number: None,
            created_from: None,
            created_to: None,
            page: 1,
            limit: 10,
        })
        .await?;
    assert!(total >= 1);
    assert!(!items.is_empty());

    // Phone number filtering works.
    let (phone_items, phone_total) = repo
        .list_instances(SmInstanceFilter {
            kind: Some("KYC_PHONE_OTP".to_owned()),
            status: None,
            user_id: None,
            phone_number: Some("+237690000001".to_owned()),
            created_from: None,
            created_to: None,
            page: 1,
            limit: 10,
        })
        .await?;
    assert!(phone_total >= 1);
    assert!(!phone_items.is_empty());

    // Cleanup.
    {
        let mut conn = pool.get().await?;
        diesel::delete(sm_instance::table.filter(sm_instance::user_id.eq(&user_id)))
            .execute(&mut conn)
            .await?;
        diesel::delete(app_user::table.filter(app_user::user_id.eq(&user_id)))
            .execute(&mut conn)
            .await?;
    }

    Ok(())
}

#[tokio::test]
async fn sm_step_attempt_lifecycle() -> Result<()> {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            eprintln!("Skipping backend-repository state machine test because DATABASE_URL is not set");
            return Ok(());
        }
    };

    let pool = connect_postgres_and_migrate(&database_url).await?;
    let repo = StateMachineRepository::new(pool.clone());

    let user_id = backend_id::user_id()?;
    {
        let mut conn = pool.get().await?;
        diesel::insert_into(app_user::table)
            .values((
                app_user::user_id.eq(&user_id),
                app_user::realm.eq("test"),
                app_user::username.eq("test-user"),
                app_user::disabled.eq(false),
                app_user::email_verified.eq(true),
                app_user::created_at.eq(Utc::now()),
                app_user::updated_at.eq(Utc::now()),
            ))
            .execute(&mut conn)
            .await?;
    }

    let instance = repo
        .create_instance(SmInstanceCreateInput {
            id: backend_id::sm_instance_id()?,
            kind: "KYC_PHONE_OTP".to_owned(),
            user_id: Some(user_id.clone()),
            idempotency_key: format!("KYC_PHONE_OTP:{user_id}:attempts"),
            status: "ACTIVE".to_owned(),
            context: json!({}),
        })
        .await?;

    let attempt = repo
        .create_step_attempt(SmStepAttemptCreateInput {
            id: backend_id::sm_attempt_id()?,
            instance_id: instance.id.clone(),
            step_name: "ISSUE_OTP".to_owned(),
            attempt_no: 1,
            status: "QUEUED".to_owned(),
            external_ref: Some("otp_ref_1".to_owned()),
            input: json!({"msisdn": "+237690000000"}),
            output: None,
            error: None,
            queued_at: Some(Utc::now()),
            started_at: None,
            finished_at: None,
            next_retry_at: None,
        })
        .await?;

    let latest = repo
        .get_latest_step_attempt(&instance.id, "ISSUE_OTP")
        .await?
        .expect("latest attempt");
    assert_eq!(latest.id, attempt.id);

    // Cleanup.
    {
        let mut conn = pool.get().await?;
        diesel::delete(sm_instance::table.filter(sm_instance::id.eq(&instance.id)))
            .execute(&mut conn)
            .await?;
        diesel::delete(app_user::table.filter(app_user::user_id.eq(&user_id)))
            .execute(&mut conn)
            .await?;
    }

    Ok(())
}
