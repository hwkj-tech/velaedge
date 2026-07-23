use std::env;

use anyhow::{bail, Result};
use axum::{
    extract::{Request, State},
    http::{header, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

const MIN_TOKEN_LENGTH: usize = 24;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiRole {
    Viewer,
    Operator,
    Admin,
}

impl ApiRole {
    fn allows(self, required: Self) -> bool {
        self >= required
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApiPrincipal {
    pub subject: String,
    pub role: ApiRole,
    pub authentication_enabled: bool,
}

#[derive(Clone)]
struct ApiCredential {
    token_digest: [u8; 32],
    principal: ApiPrincipal,
}

#[derive(Clone)]
pub struct ApiAuthConfig {
    enabled: bool,
    credentials: Vec<ApiCredential>,
}

impl ApiAuthConfig {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            credentials: Vec::new(),
        }
    }

    pub fn required(credentials: Vec<(String, ApiRole, String)>) -> Result<Self> {
        if credentials.is_empty() {
            bail!("required API authentication needs at least one credential");
        }

        let mut configured = Vec::with_capacity(credentials.len());
        for (subject, role, token) in credentials {
            let subject = subject.trim();
            if subject.is_empty() {
                bail!("API credential subject must not be empty");
            }
            if subject.chars().count() > 120 {
                bail!("API credential subject must not exceed 120 characters");
            }
            if token.len() < MIN_TOKEN_LENGTH {
                bail!("API bearer tokens must contain at least {MIN_TOKEN_LENGTH} characters");
            }
            if token.chars().any(char::is_whitespace) {
                bail!("API bearer tokens must not contain whitespace");
            }
            let token_digest = token_digest(&token);
            if configured.iter().any(|credential: &ApiCredential| {
                constant_time_eq(&credential.token_digest, &token_digest)
            }) {
                bail!("API bearer tokens must be unique");
            }
            configured.push(ApiCredential {
                token_digest,
                principal: ApiPrincipal {
                    subject: subject.to_string(),
                    role,
                    authentication_enabled: true,
                },
            });
        }

        Ok(Self {
            enabled: true,
            credentials: configured,
        })
    }

    pub fn from_env() -> Result<Self> {
        let mode = env::var("EDGEOPS_API_AUTH_MODE").unwrap_or_else(|_| "disabled".to_string());
        match mode.trim().to_ascii_lowercase().as_str() {
            "disabled" => Ok(Self::disabled()),
            "required" => {
                let mut credentials = Vec::new();
                append_env_credential(&mut credentials, "VIEWER", ApiRole::Viewer)?;
                append_env_credential(&mut credentials, "OPERATOR", ApiRole::Operator)?;
                append_env_credential(&mut credentials, "ADMIN", ApiRole::Admin)?;
                Self::required(credentials)
            }
            value => bail!("EDGEOPS_API_AUTH_MODE must be 'disabled' or 'required', got '{value}'"),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn authenticate(&self, token: &str) -> Option<ApiPrincipal> {
        let digest = token_digest(token);
        self.credentials
            .iter()
            .find(|credential| constant_time_eq(&credential.token_digest, &digest))
            .map(|credential| credential.principal.clone())
    }
}

impl Default for ApiAuthConfig {
    fn default() -> Self {
        Self::disabled()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatusResponse {
    pub subject: String,
    pub role: ApiRole,
    pub authentication_enabled: bool,
}

pub async fn authorize_api_request(
    State(config): State<ApiAuthConfig>,
    mut request: Request,
    next: Next,
) -> Response {
    let principal = if config.enabled {
        let Some(token) = bearer_token(request.headers().get(header::AUTHORIZATION)) else {
            return authentication_error(
                StatusCode::UNAUTHORIZED,
                "missing or invalid bearer token",
                true,
            );
        };
        let Some(principal) = config.authenticate(token) else {
            return authentication_error(
                StatusCode::UNAUTHORIZED,
                "missing or invalid bearer token",
                true,
            );
        };
        principal
    } else {
        ApiPrincipal {
            subject: "local-development".to_string(),
            role: ApiRole::Admin,
            authentication_enabled: false,
        }
    };

    let required_role = required_role(request.method(), request.uri().path());
    if !principal.role.allows(required_role) {
        return authentication_error(
            StatusCode::FORBIDDEN,
            "the authenticated principal does not have permission for this operation",
            false,
        );
    }

    request.extensions_mut().insert(principal);
    next.run(request).await
}

pub async fn auth_status(
    axum::Extension(principal): axum::Extension<ApiPrincipal>,
) -> Json<AuthStatusResponse> {
    Json(AuthStatusResponse {
        subject: principal.subject,
        role: principal.role,
        authentication_enabled: principal.authentication_enabled,
    })
}

fn required_role(method: &Method, path: &str) -> ApiRole {
    if method == Method::GET || method == Method::HEAD || method == Method::OPTIONS {
        ApiRole::Viewer
    } else if method == Method::DELETE
        || path.ends_with("/access-token")
        || is_agent_proposal_review(method, path)
    {
        ApiRole::Admin
    } else {
        ApiRole::Operator
    }
}

fn is_agent_proposal_review(method: &Method, path: &str) -> bool {
    *method == Method::POST
        && path.starts_with("/api/agent/proposals/")
        && (path.ends_with("/approve") || path.ends_with("/reject"))
}

fn bearer_token(value: Option<&HeaderValue>) -> Option<&str> {
    let value = value?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") || token.is_empty() || token.contains(' ') {
        return None;
    }
    Some(token)
}

fn append_env_credential(
    credentials: &mut Vec<(String, ApiRole, String)>,
    name: &str,
    role: ApiRole,
) -> Result<()> {
    let token_name = format!("EDGEOPS_{name}_TOKEN");
    let Ok(token) = env::var(&token_name) else {
        return Ok(());
    };
    let subject_name = format!("EDGEOPS_{name}_SUBJECT");
    let subject = env::var(subject_name).unwrap_or_else(|_| name.to_ascii_lowercase());
    credentials.push((subject, role, token));
    Ok(())
}

fn token_digest(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

fn constant_time_eq(left: &[u8; 32], right: &[u8; 32]) -> bool {
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

fn authentication_error(status: StatusCode, message: &str, challenge: bool) -> Response {
    let mut response = (
        status,
        Json(serde_json::json!({
            "error": message,
        })),
    )
        .into_response();
    if challenge {
        response.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            HeaderValue::from_static("Bearer realm=\"edgeops\""),
        );
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_auth_rejects_weak_and_duplicate_tokens() {
        assert!(ApiAuthConfig::required(vec![(
            "admin".to_string(),
            ApiRole::Admin,
            "too-short".to_string(),
        )])
        .is_err());

        let duplicate = "same-token-value-for-two-principals".to_string();
        assert!(ApiAuthConfig::required(vec![
            ("viewer".to_string(), ApiRole::Viewer, duplicate.clone()),
            ("admin".to_string(), ApiRole::Admin, duplicate),
        ])
        .is_err());
    }
}
