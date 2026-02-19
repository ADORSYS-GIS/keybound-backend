use axum::Router;
use axum::routing::get;
use hyper::StatusCode;

pub fn health_router() -> Router {
    Router::new().route("/health", get(health_handler))
}

async fn health_handler() -> StatusCode {
    StatusCode::OK
}
