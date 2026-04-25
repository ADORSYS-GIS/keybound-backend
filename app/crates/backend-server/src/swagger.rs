use std::iter;

use utoipa::openapi::security::SecurityRequirement;
use utoipa::openapi::security::{Http, HttpAuthScheme, SecurityScheme};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::api::{
    auth::AuthOpenApi, bff_flow::BffFlowOpenApi, bff_uploads::BffUploadsOpenApi,
    staff_flow::StaffFlowOpenApi,
};

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

pub fn swagger_ui() -> SwaggerUi {
    // Create the specs
    let mut bff_spec = BffFlowOpenApi::openapi();
    let mut uploads_spec = BffUploadsOpenApi::openapi();
    let mut staff_spec = StaffFlowOpenApi::openapi();
    let mut auth_spec = AuthOpenApi::openapi();

    // Create the Bearer HTTP security scheme
    let bearer_scheme = SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer));

    // Helper to create security requirement
    let make_req = || SecurityRequirement::new("bearerAuth", iter::empty::<String>());

    // Add security scheme and requirement to BFF spec
    if let Some(components) = bff_spec.components.as_mut() {
        components
            .security_schemes
            .insert("bearerAuth".to_string(), bearer_scheme.clone());
    }
    bff_spec.security = Some(vec![make_req()]);

    // Add security scheme and requirement to uploads spec
    if let Some(components) = uploads_spec.components.as_mut() {
        components
            .security_schemes
            .insert("bearerAuth".to_string(), bearer_scheme.clone());
    }
    uploads_spec.security = Some(vec![make_req()]);

    // Add security scheme and requirement to staff spec
    if let Some(components) = staff_spec.components.as_mut() {
        components
            .security_schemes
            .insert("bearerAuth".to_string(), bearer_scheme.clone());
    }
    staff_spec.security = Some(vec![make_req()]);

    // Add security scheme and requirement to auth spec
    if let Some(components) = auth_spec.components.as_mut() {
        components
            .security_schemes
            .insert("bearerAuth".to_string(), bearer_scheme);
    }
    auth_spec.security = Some(vec![make_req()]);

    SwaggerUi::new("/swagger-ui/")
        .url("/api-docs/bff/openapi.json", bff_spec)
        .url("/api-docs/uploads/openapi.json", uploads_spec)
        .url("/api-docs/staff/openapi.json", staff_spec)
        .url("/api-docs/auth/openapi.json", auth_spec)
        .url("/api-docs/core/openapi.json", ApiDoc::openapi())
}
