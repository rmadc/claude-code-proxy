use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use claude_code_proxy::{Registry, config::AliasProvider, server};
use std::sync::Arc;
use tower::ServiceExt;

// A separate integration-test process keeps environment mutation isolated.
#[tokio::test]
async fn actual_router_rejects_unauthenticated_requests_and_invalid_configuration() {
    let token = "synthetic-test-token-000000000000000";
    let registry = || Arc::new(Registry::from_providers(AliasProvider::Codex, []));
    unsafe {
        std::env::set_var("CCP_CLIENT_TOKEN", token);
    }
    let app = server::app(registry());
    for path in [
        "/healthz",
        "/monitor",
        "/v1/models",
        "/v1/models/gpt-6-sol",
        "/v1/codex/models",
        "/v1/messages",
        "/v1/responses",
        "/unknown",
    ] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
    }
    let response = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .header("x-api-key", token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    unsafe {
        std::env::set_var("CCP_CLIENT_TOKEN", "");
    }
    let response = server::app(registry())
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(server::bind_proxy_listener("0.0.0.0", 0).await.is_err());
    unsafe {
        std::env::remove_var("CCP_CLIENT_TOKEN");
    }
    assert!(server::bind_proxy_listener("0.0.0.0", 0).await.is_err());
    assert!(server::bind_proxy_listener("127.0.0.1", 0).await.is_ok());
}
