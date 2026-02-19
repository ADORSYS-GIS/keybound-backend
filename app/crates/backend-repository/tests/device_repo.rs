use anyhow::Result;
use backend_migrate::connect_postgres_and_migrate;
use backend_model::kc::{DeviceLookupRequest, EnrollmentBindRequest, KcAnyMap, device_record_id};
use backend_model::schema::{app_user, device};
use backend_repository::{DeviceRepo, DeviceRepository};
use chrono::Utc;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use gen_oas_server_kc::types::Object;
use serde_json::json;
use std::time::Duration;
use tokio::time::sleep;

fn build_public_jwk() -> KcAnyMap {
    let mut map = KcAnyMap::new();
    map.insert("kty".to_string(), Object(json!("EC")));
    map.insert("crv".to_string(), Object(json!("P-256")));
    map.insert("x".to_string(), Object(json!("x-value")));
    map.insert("y".to_string(), Object(json!("y-value")));
    map
}

#[tokio::test]
async fn lookup_tracks_last_seen() -> Result<()> {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            eprintln!("Skipping backend-repository device test because DATABASE_URL is not set");
            return Ok(());
        }
    };

    let pool = connect_postgres_and_migrate(&database_url).await?;
    let repo = DeviceRepository::new(pool.clone());

    let user_id = backend_id::user_id()?.to_string();
    let device_id = backend_id::device_id()?;
    let jkt = "test-jkt".to_string();

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

    let req = EnrollmentBindRequest {
        realm: "test".to_string(),
        client_id: "test-client".to_string(),
        user_id: user_id.clone(),
        user_hint: None,
        device_id: device_id.clone(),
        jkt: jkt.clone(),
        public_jwk: build_public_jwk(),
        attributes: None,
        created_at: None,
        proof: None,
    };

    let record_id = repo.bind_device(&req).await?;

    let before_lookup = Utc::now();
    sleep(Duration::from_millis(10)).await;

    let lookup_req = DeviceLookupRequest {
        device_id: Some(device_id.clone()),
        jkt: Some(jkt.clone()),
    };
    let device_row = repo
        .lookup_device(&lookup_req)
        .await?
        .expect("bound device should be found");

    let last_seen = device_row
        .last_seen_at
        .expect("lookup should set last_seen_at");

    assert!(last_seen >= before_lookup);
    assert_eq!(
        record_id,
        device_record_id(&device_id, &device_row.public_jwk),
        "bind_device should return the deterministic composite record identifier"
    );

    {
        let mut conn = pool.get().await?;
        diesel::delete(device::table.filter(device::device_id.eq(&device_id)))
            .execute(&mut conn)
            .await?;
        diesel::delete(app_user::table.filter(app_user::user_id.eq(&user_id)))
            .execute(&mut conn)
            .await?;
    }

    Ok(())
}
