use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header::AUTHORIZATION};
use axum::extract::OriginalUri;
use axum::response::{IntoResponse, Response};
use backend_core::{BffAuth, KcAuth, StaffAuth};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::hmac;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{SystemTime, UNIX_EPOCH};
use tower::{Layer, Service};

pub async fn require_kc_signature(
    cfg: &KcAuth,
    req: Request<Body>,
) -> std::result::Result<Request<Body>, Response> {
    if !cfg.enabled {
        return Ok(req);
    }

    let timestamp_header = match header_value(req.headers(), "x-kc-timestamp") {
        Some(value) => value,
        None => return Err(unauthorized("missing x-kc-timestamp")),
    };
    let signature_header = match header_value(req.headers(), "x-kc-signature") {
        Some(value) => value,
        None => return Err(unauthorized("missing x-kc-signature")),
    };
    if is_timestamp_invalid(&timestamp_header, cfg.max_clock_skew_seconds) {
        return Err(unauthorized("invalid x-kc-timestamp"));
    }

    let method = req.method().as_str().to_uppercase();
    let path = req.uri().path().to_owned();
    let (parts, body) = req.into_parts();
    let body_bytes = match to_bytes(body, cfg.max_body_bytes).await {
        Ok(value) => value,
        Err(_) => return Err(unauthorized("invalid request body")),
    };
    let body_str = String::from_utf8_lossy(&body_bytes);
    let payload = format!("{timestamp_header}\n{method}\n{path}\n{body_str}");

    let key = hmac::Key::new(hmac::HMAC_SHA256, cfg.signature_secret.as_bytes());
    let digest = hmac::sign(&key, payload.as_bytes());
    let expected = URL_SAFE_NO_PAD.encode(digest.as_ref());
    if expected != signature_header {
        return Err(unauthorized("invalid x-kc-signature"));
    }

    Ok(Request::from_parts(parts, Body::from(body_bytes)))
}

pub async fn require_bff_auth(
    cfg: &BffAuth,
    req: Request<Body>,
) -> std::result::Result<Request<Body>, Response> {
    require_user_bearer_auth(cfg.enabled, &cfg.base_path, req).await
}

pub async fn require_staff_bearer(
    cfg: &StaffAuth,
    req: Request<Body>,
) -> std::result::Result<Request<Body>, Response> {
    require_user_bearer_auth(cfg.enabled, &cfg.base_path, req).await
}

pub fn kc_signature_layer(cfg: KcAuth) -> KcSignatureLayer {
    KcSignatureLayer::new(cfg)
}

pub fn bff_bearer_layer(cfg: BffAuth) -> BffBearerLayer {
    BffBearerLayer::new(cfg)
}

pub fn staff_bearer_layer(cfg: StaffAuth) -> StaffBearerLayer {
    StaffBearerLayer::new(cfg)
}

#[derive(Clone)]
pub struct KcSignatureLayer {
    cfg: Arc<KcAuth>,
}

impl KcSignatureLayer {
    fn new(cfg: KcAuth) -> Self {
        Self { cfg: Arc::new(cfg) }
    }
}

impl<S> Layer<S> for KcSignatureLayer {
    type Service = KcSignatureService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        KcSignatureService {
            inner,
            cfg: Arc::clone(&self.cfg),
        }
    }
}

#[derive(Clone)]
pub struct KcSignatureService<S> {
    inner: S,
    cfg: Arc<KcAuth>,
}

impl<S> Service<Request<Body>> for KcSignatureService<S>
where
    S: Service<Request<Body>, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let cfg = Arc::clone(&self.cfg);
        let mut inner = self.inner.clone();
        Box::pin(async move {
            match require_kc_signature(&cfg, req).await {
                Ok(req) => inner.call(req).await,
                Err(resp) => Ok(resp),
            }
        })
    }
}

#[derive(Clone)]
pub struct BffBearerLayer {
    cfg: Arc<BffAuth>,
}

impl BffBearerLayer {
    fn new(cfg: BffAuth) -> Self {
        Self { cfg: Arc::new(cfg) }
    }
}

impl<S> Layer<S> for BffBearerLayer {
    type Service = BffBearerService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        BffBearerService {
            inner,
            cfg: Arc::clone(&self.cfg),
        }
    }
}

#[derive(Clone)]
pub struct BffBearerService<S> {
    inner: S,
    cfg: Arc<BffAuth>,
}

impl<S> Service<Request<Body>> for BffBearerService<S>
where
    S: Service<Request<Body>, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let cfg = Arc::clone(&self.cfg);
        let mut inner = self.inner.clone();
        Box::pin(async move {
            match require_bff_auth(&cfg, req).await {
                Ok(req) => inner.call(req).await,
                Err(resp) => Ok(resp),
            }
        })
    }
}

#[derive(Clone)]
pub struct StaffBearerLayer {
    cfg: Arc<StaffAuth>,
}

impl StaffBearerLayer {
    fn new(cfg: StaffAuth) -> Self {
        Self { cfg: Arc::new(cfg) }
    }
}

impl<S> Layer<S> for StaffBearerLayer {
    type Service = StaffBearerService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        StaffBearerService {
            inner,
            cfg: Arc::clone(&self.cfg),
        }
    }
}

#[derive(Clone)]
pub struct StaffBearerService<S> {
    inner: S,
    cfg: Arc<StaffAuth>,
}

impl<S> Service<Request<Body>> for StaffBearerService<S>
where
    S: Service<Request<Body>, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let cfg = Arc::clone(&self.cfg);
        let mut inner = self.inner.clone();
        Box::pin(async move {
            match require_staff_bearer(&cfg, req).await {
                Ok(req) => inner.call(req).await,
                Err(resp) => Ok(resp),
            }
        })
    }
}

#[derive(Debug, Clone)]
struct JwtClaims {
    jkt: Option<String>,
}

fn decode_jwt_claims(token: &str) -> Option<JwtClaims> {
    let mut parts = token.split('.');
    let _header = parts.next()?;
    let payload = parts.next()?;
    let payload = URL_SAFE_NO_PAD.decode(payload).ok()?;
    let payload: Value = serde_json::from_slice(&payload).ok()?;
    let jkt = payload
        .get("cnf")
        .and_then(|value| value.get("jkt"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    Some(JwtClaims { jkt })
}

fn bearer_token(headers: &axum::http::HeaderMap) -> Option<String> {
    let value = headers.get(AUTHORIZATION)?.to_str().ok()?;
    let mut parts = value.splitn(2, ' ');
    let scheme = parts.next()?;
    let token = parts.next()?;
    if scheme.eq_ignore_ascii_case("bearer") && !token.is_empty() {
        Some(token.to_owned())
    } else {
        None
    }
}

fn header_value(headers: &axum::http::HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned)
}

fn is_timestamp_invalid(timestamp: &str, max_clock_skew_seconds: i64) -> bool {
    let Ok(ts) = timestamp.parse::<i64>() else {
        return true;
    };
    let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return true;
    };
    (now.as_secs() as i64 - ts).abs() > max_clock_skew_seconds
}

fn unauthorized(message: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        axum::Json(serde_json::json!({
            "error": "unauthorized",
            "message": message
        })),
    )
        .into_response()
}

async fn require_user_bearer_auth(
    enabled: bool,
    protected_base_path: &str,
    req: Request<Body>,
) -> std::result::Result<Request<Body>, Response> {
    if !enabled {
        return Ok(req);
    }

    if protected_base_path.trim().is_empty() {
        return Ok(req);
    }

    let path = req
        .extensions()
        .get::<OriginalUri>()
        .map(|uri| uri.0.path())
        .unwrap_or_else(|| req.uri().path());

    if !path.starts_with(protected_base_path) {
        return Ok(req);
    }

    let token = match bearer_token(req.headers()) {
        Some(value) => value,
        None => return Err(unauthorized("missing bearer token")),
    };

    match decode_jwt_claims(&token) {
        Some(_) => Ok(req),
        None => Err(unauthorized("invalid bearer token")),
    }
}
