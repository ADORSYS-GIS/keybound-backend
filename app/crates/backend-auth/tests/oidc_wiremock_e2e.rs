use backend_auth::{HttpClient, OidcState};
use serde_json::json;
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn oidc_state_fetches_discovery_and_jwks_via_http() {
    let mock_server = MockServer::start().await;

    let discovery_path = "/.well-known/openid-configuration";
    let jwks_path = "/jwks";

    Mock::given(method("GET"))
        .and(path(discovery_path))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "issuer": mock_server.uri(),
            "authorization_endpoint": format!("{}/protocol/openid-connect/auth", mock_server.uri()),
            "token_endpoint": format!("{}/protocol/openid-connect/token", mock_server.uri()),
            "jwks_uri": format!("{}{}", mock_server.uri(), jwks_path)
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path(jwks_path))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"keys": []})))
        .expect(1)
        .mount(&mock_server)
        .await;

    let oidc_state = OidcState::new(
        mock_server.uri(),
        None,
        Duration::from_secs(300),
        Duration::from_secs(300),
        HttpClient::new_with_defaults().expect("http client"),
    );

    let first = oidc_state.get_jwks().await.expect("first jwks fetch");
    assert!(first.keys.is_empty());

    let second = oidc_state.get_jwks().await.expect("cached jwks fetch");
    assert!(second.keys.is_empty());
}
