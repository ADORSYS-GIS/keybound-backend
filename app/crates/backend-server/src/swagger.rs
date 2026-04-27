use std::iter;

use utoipa::OpenApi;
use utoipa::openapi::Server;
use utoipa::openapi::security::SecurityRequirement;
use utoipa::openapi::security::{
    ClientCredentials, Flow, Http, HttpAuthScheme, OAuth2, Scopes, SecurityScheme,
};
use utoipa_swagger_ui::SwaggerUi;

use crate::api::{
    auth::AuthOpenApi, bff_flow::BffFlowOpenApi, bff_uploads::BffUploadsOpenApi,
    staff_flow::StaffFlowOpenApi,
};
use backend_core::config::Config;

/// Build OAuth2 token URL from OIDC issuer URL
fn build_token_url_from_issuer(issuer: &str) -> String {
    let issuer = issuer.trim();
    if issuer.ends_with('/') {
        format!("{}protocol/openid-connect/token", issuer)
    } else {
        format!("{}/protocol/openid-connect/token", issuer)
    }
}

/// Main API documentation
#[derive(OpenApi)]
#[openapi(
    info(
        title = "KYC Tokenization Backend API",
        version = "0.2.4",
        description = "KYC orchestration backend with signature auth, flows, and webhook integration"
    ),
    tags(
        (name = "users", description = "User profile endpoints"),
        (name = "sessions", description = "Session management endpoints"),
        (name = "flows", description = "Flow execution endpoints"),
        (name = "steps", description = "Step submission endpoints"),
    )
)]
pub struct ApiDoc;

/// Creates a SwaggerUi with OAuth2 and server URLs configured from the app config
pub fn swagger_ui(config: &Config) -> SwaggerUi {
    // Create the specs
    let mut bff_spec = BffFlowOpenApi::openapi();
    let mut uploads_spec = BffUploadsOpenApi::openapi();
    let mut staff_spec = StaffFlowOpenApi::openapi();
    let mut auth_spec = AuthOpenApi::openapi();

    // Create the Bearer HTTP security scheme
    let bearer_scheme = SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer));

    // Build host URL from swagger config or fall back to server config
    let http_host = if let Some(http_host) = &config.swagger.http_host {
        http_host.clone()
    } else {
        format!("http://{}:{}", config.server.address, config.server.port)
    };

    // Build base URLs from BFF and Staff config
    let bff_base = config.bff.base_path.trim();
    let staff_base = config.staff.base_path.trim();
    let auth_base = config.auth.base_path.trim();

    // Build token URL from swagger oauth2_client config or fall back to issuer
    let token_url = if let Some(ref oauth2_client) = config.swagger.oauth2_client {
        if let Some(ref url) = oauth2_client.token_url {
            url.clone()
        } else {
            // Fall back to computing from issuer
            build_token_url_from_issuer(&config.oauth2.issuer)
        }
    } else {
        build_token_url_from_issuer(&config.oauth2.issuer)
    };

    // Helper to create security requirements
    let make_bearer_req = || SecurityRequirement::new("bearerAuth", iter::empty::<String>());
    let make_oauth2_req = || SecurityRequirement::new("keycloak", iter::empty::<String>());

    // Build OAuth2 scheme with configured token_url from swagger config
    let oauth2_scheme = SecurityScheme::OAuth2(OAuth2::new([Flow::ClientCredentials(
        ClientCredentials::new(&token_url, Scopes::new()),
    )]));

    // Add security scheme and requirement to BFF spec
    if let Some(components) = bff_spec.components.as_mut() {
        components
            .security_schemes
            .insert("bearerAuth".to_string(), bearer_scheme.clone());
        components
            .security_schemes
            .insert("keycloak".to_string(), oauth2_scheme.clone());
    }
    bff_spec.security = Some(vec![make_bearer_req(), make_oauth2_req()]);
    // Add server URL with /bff prefix so paths resolve correctly
    bff_spec.servers = Some(vec![Server::new(&format!("{}{}", http_host, bff_base))]);

    // Add security scheme and requirement to uploads spec
    if let Some(components) = uploads_spec.components.as_mut() {
        components
            .security_schemes
            .insert("bearerAuth".to_string(), bearer_scheme.clone());
        components
            .security_schemes
            .insert("keycloak".to_string(), oauth2_scheme.clone());
    }
    uploads_spec.security = Some(vec![make_bearer_req(), make_oauth2_req()]);
    uploads_spec.servers = Some(vec![Server::new(&http_host)]);

    // Add security scheme and requirement to staff spec
    if let Some(components) = staff_spec.components.as_mut() {
        components
            .security_schemes
            .insert("bearerAuth".to_string(), bearer_scheme.clone());
        components
            .security_schemes
            .insert("keycloak".to_string(), oauth2_scheme.clone());
    }
    staff_spec.security = Some(vec![make_bearer_req(), make_oauth2_req()]);
    staff_spec.servers = Some(vec![Server::new(&format!("{}{}", http_host, staff_base))]);

    // Add security scheme and requirement to auth spec
    if let Some(components) = auth_spec.components.as_mut() {
        components
            .security_schemes
            .insert("bearerAuth".to_string(), bearer_scheme);
        components
            .security_schemes
            .insert("keycloak".to_string(), oauth2_scheme);
    }
    auth_spec.security = Some(vec![make_bearer_req(), make_oauth2_req()]);
    auth_spec.servers = Some(vec![Server::new(&format!("{}{}", http_host, auth_base))]);

    SwaggerUi::new("/swagger-ui/")
        .url("/api-docs/bff/openapi.json", bff_spec)
        .url("/api-docs/uploads/openapi.json", uploads_spec)
        .url("/api-docs/staff/openapi.json", staff_spec)
        .url("/api-docs/auth/openapi.json", auth_spec)
        .url("/api-docs/core/openapi.json", ApiDoc::openapi())
}
