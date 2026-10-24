use async_trait::async_trait;
use axum::{
    Json,
    body::Body,
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
};
use claude_code_proxy::{
    CliHandlers, MessagesRequest, Provider, Registry, RequestContext, config::AliasProvider, paths,
    server,
};
use serde_json::json;
use std::{fs, sync::Arc};
use tower::ServiceExt;

struct ErrorProvider;
static ERROR_PROVIDER: ErrorProvider = ErrorProvider;

impl CliHandlers for ErrorProvider {
    fn login(&self) -> anyhow::Result<()> {
        unreachable!("test must not sign in")
    }
    fn device(&self) -> anyhow::Result<()> {
        unreachable!("test must not sign in")
    }
    fn status(&self) -> anyhow::Result<()> {
        unreachable!("test must not read credentials")
    }
    fn logout(&self) -> anyhow::Result<()> {
        unreachable!("test must not modify credentials")
    }
}

#[async_trait]
impl Provider for ErrorProvider {
    fn name(&self) -> &'static str {
        "synthetic"
    }
    fn supported_models(&self) -> Vec<String> {
        vec!["security-test".into()]
    }
    fn cli(&self) -> &'static dyn CliHandlers {
        &ERROR_PROVIDER
    }
    async fn handle_messages(&self, _: MessagesRequest, _: RequestContext) -> Response {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": {
                "message":"synthetic-sensitive-prose",
                "accessToken":"synthetic-sensitive-token",
                "refresh_token":"synthetic-refresh-token"
            }})),
        )
            .into_response()
    }
    async fn handle_count_tokens(&self, body: MessagesRequest, ctx: RequestContext) -> Response {
        self.handle_messages(body, ctx).await
    }
}

// Separate process: these environment changes cannot affect other test binaries.
#[tokio::test]
async fn error_content_requires_opt_in_and_known_secrets_are_redacted() {
    let temp = tempfile::tempdir().unwrap();
    unsafe {
        for name in [
            "CCP_ERROR_CAPTURE",
            "CCP_LOG_CONTENT",
            "CCP_CLIENT_TOKEN",
            "CCP_TRAFFIC_LOG",
        ] {
            std::env::remove_var(name);
        }
        for name in ["CCP_CONFIG_DIR", "LOCALAPPDATA", "XDG_STATE_HOME"] {
            std::env::set_var(name, temp.path());
        }
    }
    let registry = Registry::from_providers(
        AliasProvider::Codex,
        [Arc::new(ErrorProvider) as Arc<dyn Provider>],
    );
    let app = server::app(Arc::new(registry));
    let request = || {
        Request::builder()
            .method("POST")
            .uri("/v1/messages")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"model":"security-test","messages":[],"max_tokens":1}).to_string(),
            ))
            .unwrap()
    };
    assert_eq!(
        app.clone().oneshot(request()).await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );
    assert!(!paths::state_dir().join("errors").exists());
    let log = fs::read_to_string(paths::log_file()).unwrap();
    assert!(!log.contains("synthetic-sensitive-prose"));
    assert!(!log.contains("synthetic-sensitive-token"));
    unsafe {
        std::env::set_var("CCP_ERROR_CAPTURE", "1");
    }
    assert_eq!(
        app.oneshot(request()).await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );
    let capture = fs::read_dir(paths::state_dir().join("errors"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let capture = fs::read_to_string(capture).unwrap();
    assert!(capture.contains("synthetic-sensitive-prose")); // Opt-in retains prose.
    assert!(!capture.contains("synthetic-sensitive-token"));
    assert!(!capture.contains("synthetic-refresh-token"));
}
