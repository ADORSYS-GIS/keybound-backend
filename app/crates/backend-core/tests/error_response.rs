use axum::body::to_bytes;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use backend_core::Error;
use serde_json::json;

#[test]
fn unauthorized_helper_builds_expected_http_error() {
    let error = Error::unauthorized("missing token");
    let meta = error.meta();

    assert_eq!(meta.error_key, "UNAUTHORIZED");
    assert_eq!(meta.status_code, 401);
    assert_eq!(meta.message, "missing token");
    assert!(meta.context.is_none());
}

#[test]
fn database_error_maps_to_database_error_meta() {
    let error = Error::Database("db down".to_owned());
    let meta = error.meta();

    assert_eq!(meta.error_key, "DATABASE_ERROR");
    assert_eq!(meta.status_code, 500);
    assert_eq!(meta.message, "Database operation failed");
    assert!(meta.context.is_none());
}

#[test]
fn with_context_applies_only_for_http_error() {
    let context = json!({"field": "user_id"});

    let http_error = Error::bad_request("INVALID", "invalid input").with_context(context.clone());
    let http_meta = http_error.meta();
    assert_eq!(http_meta.context, Some(context.clone()));

    let non_http_error = Error::Server("boom".to_owned()).with_context(context);
    let non_http_meta = non_http_error.meta();
    assert!(non_http_meta.context.is_none());
}

#[tokio::test]
async fn into_response_uses_http_status_and_payload_for_http_error() {
    let error = Error::conflict("DEVICE_CONFLICT", "already bound")
        .with_context(json!({"device_id": "dvc_123"}));

    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::CONFLICT);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(payload["error_key"], "DEVICE_CONFLICT");
    assert_eq!(payload["message"], "already bound");
    assert_eq!(payload["context"], json!({"device_id": "dvc_123"}));
}

#[tokio::test]
async fn into_response_falls_back_to_internal_server_error_for_invalid_http_status() {
    let error = Error::Http {
        error_key: "BROKEN_STATUS",
        status_code: 0,
        message: "broken status".to_owned(),
        context: None,
    };

    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(payload["error_key"], "BROKEN_STATUS");
    assert_eq!(payload["message"], "broken status");
    assert_eq!(payload.get("context"), None);
}

#[tokio::test]
async fn into_response_maps_non_http_errors_to_internal_server_error_payload() {
    let error = Error::Server("panic-like failure".to_owned());
    let response = error.into_response();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(payload["error_key"], "INTERNAL_SERVER_ERROR");
    assert_eq!(payload["message"], "Internal server error");
}
