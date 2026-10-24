use axum::{
    Json, Router,
    body::Body,
    http::{HeaderMap, Request, StatusCode},
    routing::get,
};
use claude_code_proxy::{
    Registry,
    config::AliasProvider,
    providers::codex::auth::token_store::{StoredAuth, file_store},
    server,
};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tower::ServiceExt;

// A dedicated process isolates environment mutations and synthetic credentials.
#[tokio::test]
async fn authenticated_catalog_drives_metadata_and_is_cached_without_leaking_raw_fields() {
    let temp = tempfile::tempdir().unwrap();
    let requests = Arc::new(AtomicUsize::new(0));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let count = requests.clone();
    let upstream = Router::new().route("/models", get(move |headers:HeaderMap| {
        let count=count.clone();
        async move {
            assert_eq!(headers["authorization"], "Bearer synthetic-provider-token");
            assert_eq!(headers["originator"], "codex_cli_rs");
            count.fetch_add(1,Ordering::SeqCst);
            Json(json!({"models":[
                {"slug":"gpt-6-sol","context_window":272000,"max_context_window":872000,"default_reasoning_level":"medium","supported_reasoning_levels":[{"effort":"medium"},{"effort":"high"}],"instructions":"synthetic-sensitive-instructions"},
                {"slug":"gpt-6.1-sol","context_window":500000,"default_reasoning_level":"low","supported_reasoning_levels":[{"effort":"low"},{"effort":"high"}]}
            ]}))
        }
    }));
    let task = tokio::spawn(async move {
        axum::serve(listener, upstream).await.unwrap();
    });
    let token = "synthetic-client-token-000000000000000";
    unsafe {
        std::env::set_var("CCP_CONFIG_DIR", temp.path());
        std::env::set_var("CCP_CODEX_BASE_URL", format!("http://{address}/responses"));
        std::env::set_var("CCP_CLIENT_TOKEN", token);
    }
    file_store()
        .save_auth(StoredAuth {
            access: "synthetic-provider-token".into(),
            refresh: "synthetic-refresh".into(),
            expires: u64::MAX,
            account_id: None,
        })
        .unwrap();
    let app = server::app(Arc::new(Registry::from_providers(AliasProvider::Codex, [])));
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/codex/models")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(requests.load(Ordering::SeqCst), 0);
    for path in ["/v1/codex/models", "/v1/models", "/v1/models/gpt-6.1-sol"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .header("x-api-key", token)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert!(!String::from_utf8_lossy(&body).contains("synthetic-sensitive-instructions"));
        if path.ends_with("gpt-6.1-sol") {
            assert_eq!(value["max_input_tokens"], 500000);
        }
    }
    assert_eq!(requests.load(Ordering::SeqCst), 1);
    task.abort();
}
