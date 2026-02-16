pub(crate) mod api;
pub(crate) mod sms_retry;
pub(crate) mod state;
pub(crate) mod worker;

use axum::body::Body;
use axum::http::Request as HttpRequest;
use axum::response::Response;
use axum::Router;
use backend_auth::{bff_bearer_layer, kc_signature_layer, staff_bearer_layer};
use backend_core::{Config, Result};
use hyper::StatusCode;
use std::convert::Infallible;
use std::sync::Arc;
use tower::service_fn;
use tower_http::trace::TraceLayer;
use tracing::info;

pub async fn serve(core_config: &Config) -> Result<()> {
    let listen_addr = core_config.api_listen_addr()?;
    let state = Arc::new(state::AppState::from_config(core_config).await?);

    let api = api::BackendApi::new(state.clone());
    let app = build_router(&api, &state.config);

    info!("Listening on {}", listen_addr);

    let handle = axum_server::Handle::new();
    let shutdown_handle = handle.clone();

    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        shutdown_handle.graceful_shutdown(None);
    });

    match core_config.api_tls_files() {
        Some((cert_path, key_path)) => {
            let rustls_config =
                axum_server::tls_rustls::RustlsConfig::from_pem_file(cert_path, key_path).await?;

            axum_server::bind_rustls(listen_addr, rustls_config)
                .handle(handle)
                .serve(app.into_make_service())
                .await?;
        }
        None => {
            axum_server::bind(listen_addr)
                .handle(handle)
                .serve(app.into_make_service())
                .await?;
        }
    }

    Ok(())
}

pub async fn run_worker(core_config: &Config) -> Result<()> {
    let state = Arc::new(state::AppState::from_config(core_config).await?);
    worker::run(state).await
}

fn build_router(api: &api::BackendApi, config: &Config) -> Router {
    // Mount sub-routers onto a fresh root router
    let mut router = Router::new();

    // Mount KC router if base path is provided
    let kc_base = config.kc.base_path.trim();
    if !kc_base.is_empty() && kc_base != "/" {
        let kc_router = build_kc_router(api.clone(), config.kc.clone());
        router = router.nest(kc_base, kc_router);
    }

    // Mount BFF router if base path is provided
    let bff_base = config.bff.base_path.trim();
    if !bff_base.is_empty() && bff_base != "/" {
        let bff_router = build_bff_router(api.clone(), config.bff.clone());
        router = router.nest(bff_base, bff_router);
    }

    // Mount Staff router if base path is provided
    let staff_base = config.staff.base_path.trim();
    if !staff_base.is_empty() && staff_base != "/" {
        let staff_router = build_staff_router(api.clone(), config.staff.clone());
        router = router.nest(staff_base, staff_router);
    }

    // 404 fallback for unmatched routes
    router = router.fallback_service(service_fn(|req| async {
        let res = Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Not Found"))
            .unwrap();
        Ok::<_, Infallible>(res)
    }));

    if config.logging.request_logging.enabled || config.logging.log_requests_enabled {
        router.layer(
            TraceLayer::new_for_http().make_span_with(|req: &HttpRequest<_>| {
                tracing::info_span!(
                    "http-request",
                    method = %req.method(),
                    path = %request_path(req)
                )
            }),
        )
    } else {
        router
    }
}

fn build_kc_router(api: api::BackendApi, cfg: backend_core::KcAuth) -> Router {
    let layer = kc_signature_layer(cfg);
    let router = gen_oas_server_kc::server::new(api);
    router.layer(layer)
}

fn build_bff_router(api: api::BackendApi, cfg: backend_core::BffAuth) -> Router {
    let layer = bff_bearer_layer(cfg);
    let router = gen_oas_server_bff::server::new(api);
    router.layer(layer)
}

fn build_staff_router(api: api::BackendApi, cfg: backend_core::StaffAuth) -> Router {
    let layer = staff_bearer_layer(cfg);
    let router = gen_oas_server_staff::server::new(api);
    router.layer(layer)
}

fn nest_surface_router<F>(router: Router, base_path: &str, build_child: F) -> Router
where
    F: FnOnce() -> Router,
{
    let trimmed = base_path.trim();
    if trimmed.is_empty() {
        return router;
    }

    if trimmed == "/" {
        return router;
    }

    router.nest(trimmed, build_child())
}

fn request_path(req: &HttpRequest<Body>) -> String {
    req.extensions()
        .get::<axum::extract::OriginalUri>()
        .map(|uri| uri.0.path().to_owned())
        .unwrap_or_else(|| req.uri().path().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use axum::http::StatusCode;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    #[tokio::test]
    async fn non_blank_base_path_mounts_service() {
        let router = nest_surface_router(Router::new(), "/api/test", || {
            Router::new().route("/", get(|| async { "mounted" }))
        });

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/api/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn blank_base_path_does_not_mount_service() {
        let router = Router::new().route("/ping", get(|| async { "pong" }));
        let router = nest_surface_router(router, "   ", || {
            panic!("should not nest when base path is blank");
        });

        let response = router
            .oneshot(Request::builder().uri("/ping").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn base_path_mounts_nested_routes() {
        let router = nest_surface_router(Router::new(), "/bff", || {
            Router::new().route("/api/kyc/cases/mine", get(|| async { "ok" }))
        });

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/bff/api/kyc/cases/mine")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn auth_middleware_requires_token_on_prefixed_path() {
        use axum::response::Response;
        use backend_auth::bff_bearer_layer;
        use backend_core::BffAuth;
        use tower::service_fn;

        let cfg = BffAuth {
            enabled: true,
            base_path: "/bff".to_string(),
        };

        let router = nest_surface_router(Router::new(), &cfg.base_path, || {
            Router::new()
                .fallback_service(service_fn(|_req| async {
                    Ok::<_, std::convert::Infallible>(Response::new(Body::from("fallback")))
                }))
                .layer(bff_bearer_layer(cfg.clone()))
        });

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/bff/api/kyc/cases/mine")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
