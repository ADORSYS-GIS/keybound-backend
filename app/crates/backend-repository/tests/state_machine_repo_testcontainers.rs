use anyhow::Result;
use backend_migrate::connect_postgres_and_migrate;
use backend_model::schema::app_user;
use backend_repository::{SmInstanceCreateInput, StateMachineRepo, StateMachineRepository};
use chrono::Utc;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use serde_json::json;
use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

#[tokio::test]
async fn state_machine_repository_runs_against_testcontainers_postgres() -> Result<()> {
    let postgres = Postgres::default().with_host_auth().start().await?;
    let database_url = format!(
        "postgres://postgres@{}:{}/postgres",
        postgres.get_host().await?,
        postgres.get_host_port_ipv4(5432).await?
    );

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

    let idempotency_key = format!("KYC_PHONE_OTP:{user_id}");
    let instance_id = backend_id::sm_instance_id()?;

    let created = repo
        .create_instance(SmInstanceCreateInput {
            id: instance_id.clone(),
            kind: "KYC_PHONE_OTP".to_owned(),
            user_id: Some(user_id),
            idempotency_key: idempotency_key.clone(),
            status: "ACTIVE".to_owned(),
            context: json!({"source":"testcontainers"}),
        })
        .await?;
    assert_eq!(created.id, instance_id);

    let fetched = repo
        .get_instance_by_idempotency_key(&idempotency_key)
        .await?
        .expect("instance should be retrievable by idempotency key");
    assert_eq!(fetched.id, created.id);

    Ok(())
}
