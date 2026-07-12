use crate::core::{current_unix_secs, OAuthDeviceAuthorization, OAuthError, OAuthTokenSet};
use crate::network::{OAuthHttpExecutor, OAuthHttpRequest};
use crate::provider::{
    ProviderOAuthAccount, ProviderOAuthAdapter, ProviderOAuthCapabilities,
    ProviderOAuthImportInput, ProviderOAuthRequestAuth, ProviderOAuthTokenSet,
    ProviderOAuthTransportContext,
};
use async_trait::async_trait;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use url::form_urlencoded;

pub const NOUS_PROVIDER_TYPE: &str = "nous";
pub const NOUS_CLIENT_ID: &str = "hermes-cli";
pub const NOUS_SCOPE: &str = "inference:invoke";
pub const NOUS_PORTAL_BASE_URL: &str = "https://portal.nousresearch.com";

#[derive(Debug, Clone)]
pub struct NousProviderOAuthAdapter {
    portal_base_url: String,
}

impl Default for NousProviderOAuthAdapter {
    fn default() -> Self {
        Self {
            portal_base_url: NOUS_PORTAL_BASE_URL.to_string(),
        }
    }
}

impl NousProviderOAuthAdapter {
    pub fn with_portal_base_url_for_tests(mut self, value: impl Into<String>) -> Self {
        self.portal_base_url = value.into();
        self
    }
    fn url(&self, path: &str) -> String {
        format!("{}{}", self.portal_base_url.trim_end_matches('/'), path)
    }

    async fn post_form(
        &self,
        executor: &dyn OAuthHttpExecutor,
        ctx: &ProviderOAuthTransportContext,
        request_id: &str,
        path: &str,
        pairs: &[(&str, &str)],
        refresh_header: Option<&str>,
    ) -> Result<Value, OAuthError> {
        let body_bytes = {
            let mut form = form_urlencoded::Serializer::new(String::new());
            for (key, value) in pairs {
                form.append_pair(key, value);
            }
            form.finish().into_bytes()
        };
        let mut headers = BTreeMap::from([
            (
                "content-type".to_string(),
                "application/x-www-form-urlencoded".to_string(),
            ),
            ("accept".to_string(), "application/json".to_string()),
        ]);
        if let Some(token) = refresh_header {
            headers.insert("x-nous-refresh-token".to_string(), token.to_string());
        }
        let response = executor
            .execute(OAuthHttpRequest {
                request_id: request_id.to_string(),
                method: reqwest::Method::POST,
                url: self.url(path),
                headers,
                content_type: Some("application/x-www-form-urlencoded".to_string()),
                json_body: None,
                body_bytes: Some(body_bytes),
                network: ctx.network.clone(),
            })
            .await?;
        let payload = response
            .json_body
            .or_else(|| serde_json::from_str(&response.body_text).ok())
            .unwrap_or(Value::Null);
        if !(200..300).contains(&response.status_code) {
            return Err(OAuthError::HttpStatus {
                status_code: response.status_code,
                body_excerpt: response.body_text.chars().take(500).collect(),
            });
        }
        Ok(payload)
    }

    pub async fn start_device_authorization(
        &self,
        executor: &dyn OAuthHttpExecutor,
        ctx: &ProviderOAuthTransportContext,
    ) -> Result<OAuthDeviceAuthorization, OAuthError> {
        let payload = self
            .post_form(
                executor,
                ctx,
                "nous_device_authorize",
                "/api/oauth/device/code",
                &[("client_id", NOUS_CLIENT_ID), ("scope", NOUS_SCOPE)],
                None,
            )
            .await?;
        serde_json::from_value(payload)
            .map_err(|_| OAuthError::invalid_response("Nous device response is invalid"))
    }

    pub async fn poll_device_authorization(
        &self,
        executor: &dyn OAuthHttpExecutor,
        ctx: &ProviderOAuthTransportContext,
        device_code: &str,
    ) -> Result<ProviderOAuthTokenSet, OAuthError> {
        let payload = self
            .post_form(
                executor,
                ctx,
                "nous_device_poll",
                "/api/oauth/token",
                &[
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ("client_id", NOUS_CLIENT_ID),
                    ("device_code", device_code),
                ],
                None,
            )
            .await?;
        self.tokens(payload, None)
    }

    async fn refresh_token(
        &self,
        executor: &dyn OAuthHttpExecutor,
        ctx: &ProviderOAuthTransportContext,
        refresh_token: &str,
    ) -> Result<ProviderOAuthTokenSet, OAuthError> {
        let payload = self
            .post_form(
                executor,
                ctx,
                "nous_refresh_token",
                "/api/oauth/token",
                &[
                    ("grant_type", "refresh_token"),
                    ("client_id", NOUS_CLIENT_ID),
                ],
                Some(refresh_token),
            )
            .await?;
        self.tokens(payload, Some(refresh_token))
    }

    fn tokens(
        &self,
        payload: Value,
        old_refresh: Option<&str>,
    ) -> Result<ProviderOAuthTokenSet, OAuthError> {
        let mut token_set =
            OAuthTokenSet::from_token_payload(payload.clone()).ok_or_else(|| {
                OAuthError::invalid_response("Nous token response missing access_token")
            })?;
        if token_set.refresh_token.is_none() {
            token_set.refresh_token = old_refresh.map(ToOwned::to_owned);
        }
        let mut config = payload.as_object().cloned().unwrap_or_default();
        config.remove("access_token");
        config.remove("agent_key");
        config.insert("provider_type".into(), json!(NOUS_PROVIDER_TYPE));
        config.insert("client_id".into(), json!(NOUS_CLIENT_ID));
        config.insert("portal_base_url".into(), json!(NOUS_PORTAL_BASE_URL));
        config.insert(
            "inference_base_url".into(),
            json!("https://inference-api.nousresearch.com/v1"),
        );
        config.insert("updated_at".into(), json!(current_unix_secs()));
        if let Some(value) = &token_set.refresh_token {
            config.insert("refresh_token".into(), json!(value));
        }
        if let Some(value) = token_set.expires_at_unix_secs {
            config.insert("expires_at".into(), json!(value));
        }
        Ok(ProviderOAuthTokenSet {
            token_set,
            auth_config: Value::Object(config),
        })
    }
}

#[async_trait]
impl ProviderOAuthAdapter for NousProviderOAuthAdapter {
    fn provider_type(&self) -> &'static str {
        NOUS_PROVIDER_TYPE
    }
    fn capabilities(&self) -> ProviderOAuthCapabilities {
        ProviderOAuthCapabilities {
            supports_authorization_code: false,
            supports_refresh_token_import: true,
            supports_batch_import: true,
            supports_device_flow: true,
            supports_account_probe: false,
            rotates_refresh_token: true,
        }
    }
    async fn import_credentials(
        &self,
        executor: &dyn OAuthHttpExecutor,
        ctx: &ProviderOAuthTransportContext,
        input: ProviderOAuthImportInput,
    ) -> Result<ProviderOAuthTokenSet, OAuthError> {
        let refresh = input
            .refresh_token
            .as_deref()
            .or_else(|| {
                input
                    .raw_credentials
                    .as_ref()
                    .and_then(|v| v.get("refresh_token"))
                    .and_then(Value::as_str)
            })
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| OAuthError::invalid_request("refresh_token is required"))?;
        self.refresh_token(executor, ctx, refresh).await
    }
    async fn refresh(
        &self,
        executor: &dyn OAuthHttpExecutor,
        ctx: &ProviderOAuthTransportContext,
        account: &ProviderOAuthAccount,
    ) -> Result<ProviderOAuthTokenSet, OAuthError> {
        let refresh = account
            .auth_config
            .get("refresh_token")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| OAuthError::invalid_request("auth_config missing refresh_token"))?;
        let mut result = self.refresh_token(executor, ctx, refresh).await?;
        if let (Some(old), Some(new)) = (
            account.auth_config.as_object(),
            result.auth_config.as_object_mut(),
        ) {
            for (k, v) in old {
                new.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
        Ok(result)
    }
    fn resolve_request_auth(
        &self,
        account: &ProviderOAuthAccount,
    ) -> Result<ProviderOAuthRequestAuth, OAuthError> {
        Ok(account.request_bearer_auth())
    }
    fn account_fingerprint(&self, account: &ProviderOAuthAccount) -> Option<String> {
        let value = account
            .auth_config
            .get("refresh_token")
            .and_then(Value::as_str)
            .unwrap_or(&account.access_token);
        let digest = Sha256::digest(value.as_bytes());
        Some(digest[..8].iter().map(|b| format!("{b:02x}")).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::{OAuthHttpResponse, OAuthNetworkContext};
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    struct Executor {
        request: Arc<Mutex<Option<OAuthHttpRequest>>>,
        response: Value,
    }
    #[async_trait]
    impl OAuthHttpExecutor for Executor {
        async fn execute(
            &self,
            request: OAuthHttpRequest,
        ) -> Result<OAuthHttpResponse, OAuthError> {
            *self.request.lock().unwrap() = Some(request);
            Ok(OAuthHttpResponse {
                status_code: 200,
                body_text: self.response.to_string(),
                json_body: Some(self.response.clone()),
            })
        }
    }
    fn context() -> ProviderOAuthTransportContext {
        ProviderOAuthTransportContext {
            provider_id: "p".into(),
            provider_type: "nous".into(),
            endpoint_id: None,
            key_id: None,
            auth_type: Some("oauth".into()),
            decrypted_api_key: None,
            decrypted_auth_config: None,
            provider_config: None,
            endpoint_config: None,
            key_config: None,
            network: OAuthNetworkContext::provider_operation(None),
        }
    }

    #[tokio::test]
    async fn refresh_uses_nous_header_and_persists_rotation() {
        let request = Arc::new(Mutex::new(None));
        let executor = Executor {
            request: request.clone(),
            response: json!({"access_token":"new-access","refresh_token":"new-refresh","expires_in":900,"token_type":"Bearer"}),
        };
        let account = ProviderOAuthAccount {
            provider_type: "nous".into(),
            access_token: "old-access".into(),
            auth_config: json!({"refresh_token":"old-refresh","scope":"inference:invoke"}),
            expires_at_unix_secs: None,
            identity: BTreeMap::new(),
        };
        let result = NousProviderOAuthAdapter::default()
            .refresh(&executor, &context(), &account)
            .await
            .unwrap();
        assert_eq!(result.auth_config["refresh_token"], "new-refresh");
        let seen = request.lock().unwrap().clone().unwrap();
        assert_eq!(seen.headers["x-nous-refresh-token"], "old-refresh");
        let body = String::from_utf8(seen.body_bytes.unwrap()).unwrap();
        assert!(body.contains("grant_type=refresh_token"));
        assert!(!body.contains("old-refresh"));
    }

    #[tokio::test]
    async fn device_authorization_matches_official_form() {
        let request = Arc::new(Mutex::new(None));
        let executor = Executor {
            request: request.clone(),
            response: json!({"device_code":"device","user_code":"CODE","verification_uri":"https://example.test/verify","verification_uri_complete":"https://example.test/verify?code=CODE","expires_in":600,"interval":5}),
        };
        let result = NousProviderOAuthAdapter::default()
            .start_device_authorization(&executor, &context())
            .await
            .unwrap();
        assert_eq!(result.device_code, "device");
        let seen = request.lock().unwrap().clone().unwrap();
        assert!(seen.url.ends_with("/api/oauth/device/code"));
        let body = String::from_utf8(seen.body_bytes.unwrap()).unwrap();
        assert!(body.contains("client_id=hermes-cli"));
        assert!(body.contains("scope=inference%3Ainvoke"));
    }
}
