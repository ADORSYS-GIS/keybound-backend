pub(crate) mod api;
pub(crate) mod sms_retry;
pub(crate) mod state;
pub(crate) mod worker;

use axum::Router;
use axum::body::Body;
use axum::http::Request as HttpRequest;
use backend_auth::{bff_bearer_layer, kc_signature_layer, staff_bearer_layer};
use backend_core::{Config, Result};
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
    let router = Router::new();
    let router = nest_surface_router(router, &config.kc.base_path, {
        let api = api.clone();
        let cfg = config.kc.clone();
        move || build_kc_router(api.clone(), cfg.clone())
    });
    let router = nest_surface_router(router, &config.bff.base_path, {
        let api = api.clone();
        let cfg = config.bff.clone();
        move || build_bff_router(api.clone(), cfg.clone())
    });
    let router = nest_surface_router(router, &config.staff.base_path, {
        let api = api.clone();
        let cfg = config.staff.clone();
        move || build_staff_router(api.clone(), cfg.clone())
    });

    if config.logging.request_logging.enabled || config.logging.log_requests_enabled {
        router.layer(TraceLayer::new_for_http().make_span_with(|req: &HttpRequest<_>| {
            tracing::info_span!(
                "http-request",
                method = %req.method(),
                path = %request_path(req)
            )
        }))
    } else {
        router
    }
}

fn build_kc_router(api: api::BackendApi, cfg: backend_core::KcAuth) -> Router {
    let layer = kc_signature_layer(cfg);
    Router::new()
        .fallback_service(service_fn(move |req| {
            let api = api.clone();
            async move { Ok(state::call_kc(api, req).await) }
        }))
        .layer(layer)
}

fn build_bff_router(api: api::BackendApi, cfg: backend_core::BffAuth) -> Router {
    let layer = bff_bearer_layer(cfg);
    Router::new()
        .fallback_service(service_fn(move |req| {
            let api = api.clone();
            async move { Ok(state::call_bff(api, req).await) }
        }))
        .layer(layer)
}

fn build_staff_router(api: api::BackendApi, cfg: backend_core::StaffAuth) -> Router {
    let layer = staff_bearer_layer(cfg);
    Router::new()
        .fallback_service(service_fn(move |req| {
            let api = api.clone();
            async move { Ok(state::call_staff(api, req).await) }
        }))
        .layer(layer)
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
    use axum::Router;
    use axum::body::Body;
    use axum::http::StatusCode;
    use axum::routing::get;
    use tower::ServiceExt;
    use axum::http::Request;

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
        use backend_core::BffAuth;
        use backend_auth::bff_bearer_layer;
        use tower::service_fn;
        use axum::response::Response;

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
