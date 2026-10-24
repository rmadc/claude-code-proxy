//! Authentication is checked before route handlers, including monitor and health.
use axum::{
    body::Body,
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

#[derive(Clone)]
pub struct ClientAuth(Option<[u8; 32]>);

impl ClientAuth {
    pub fn from_env() -> anyhow::Result<Self> {
        match std::env::var("CCP_CLIENT_TOKEN") {
            Ok(token) => Self::new(Some(&token)),
            Err(std::env::VarError::NotPresent) => Self::new(None),
            Err(_) => anyhow::bail!("CCP_CLIENT_TOKEN must be valid Unicode ASCII text"),
        }
    }

    pub fn new(token: Option<&str>) -> anyhow::Result<Self> {
        if let Some(token) = token {
            anyhow::ensure!(
                token.len() >= 32 && token.bytes().all(|b| b.is_ascii_graphic()),
                "CCP_CLIENT_TOKEN must contain at least 32 printable ASCII characters without spaces"
            );
        }
        Ok(Self(
            token.map(|token| Sha256::digest(token.as_bytes()).into()),
        ))
    }

    pub fn validate_bind(&self, ip: std::net::IpAddr, allow_network: bool) -> anyhow::Result<()> {
        anyhow::ensure!(
            ip.is_loopback() || (self.0.is_some() && allow_network),
            "Non-loopback binding requires CCP_CLIENT_TOKEN and CCP_ALLOW_NETWORK=1; use a trusted TLS tunnel for remote access"
        );
        Ok(())
    }

    fn accepts(&self, headers: &http::HeaderMap) -> bool {
        let Some(expected) = self.0 else {
            return true;
        };
        if headers.get_all("authorization").iter().count() > 1
            || headers.get_all("x-api-key").iter().count() > 1
        {
            return false;
        }
        let bearer = headers
            .get("authorization")
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.split_once(' '))
            .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("Bearer"))
            .map(|(_, token)| token);
        let api_key = headers.get("x-api-key").and_then(|h| h.to_str().ok());
        [bearer, api_key].into_iter().flatten().any(|token| {
            let actual: [u8; 32] = Sha256::digest(token.as_bytes()).into();
            bool::from(actual.ct_eq(&expected))
        })
    }
}

pub async fn require_auth(
    State(auth): State<ClientAuth>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if !auth.accepts(request.headers()) {
        return (
            StatusCode::UNAUTHORIZED,
            [("www-authenticate", "Bearer")],
            "Client authentication required",
        )
            .into_response();
    }
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, middleware, routing::get};
    use tower::ServiceExt;

    #[tokio::test]
    async fn protects_all_routes_before_handlers() {
        let token = "synthetic-test-token-000000000000000";
        let auth = ClientAuth::new(Some(token)).unwrap();
        let app = Router::new()
            .route("/healthz", get(|| async { "ok" }))
            .layer(middleware::from_fn_with_state(auth, require_auth));
        for path in ["/healthz", "/monitor", "/v1/messages", "/unknown"] {
            let response = app
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
        for name in ["authorization", "x-api-key"] {
            for valid in [false, true] {
                let value = if valid { token } else { "incorrect" };
                let value = if name == "authorization" {
                    format!("Bearer {value}")
                } else {
                    value.to_owned()
                };
                let response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .uri("/healthz")
                            .header(name, value)
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(
                    response.status(),
                    if valid {
                        StatusCode::OK
                    } else {
                        StatusCode::UNAUTHORIZED
                    }
                );
            }
        }
    }

    #[test]
    fn network_binding_requires_both_safeguards() {
        for token in [None, Some("synthetic-test-token-000000000000000")] {
            let auth = ClientAuth::new(token).unwrap();
            for ip in [
                "127.0.0.1",
                "::1",
                "0.0.0.0",
                "::",
                "192.0.2.10",
                "::ffff:127.0.0.1",
            ] {
                let ip: std::net::IpAddr = ip.parse().unwrap();
                for allow in [false, true] {
                    assert_eq!(
                        auth.validate_bind(ip, allow).is_ok(),
                        ip.is_loopback() || (token.is_some() && allow)
                    );
                }
            }
        }
        assert!(ClientAuth::new(Some("")).is_err());
        assert!(ClientAuth::new(Some("short")).is_err());
    }
}
