use std::sync::Arc;

use eyre::Result;

use crate::openai::OpenAIProvider;
use crate::openai_responses::{OpenAIResponsesProvider, is_chatgpt_subscription_model, is_responses_capable};
use crate::provider::LlmProvider;

use super::{CreateParams, CredentialKind, ProviderEntry};

pub const ENTRY: ProviderEntry = ProviderEntry {
    name: "openai",
    aliases: &[],
    api_key_env: Some("OPENAI_API_KEY"),
    key_env_aliases: &[],
    default_base_url: Some("https://api.openai.com/v1"),
    requires_api_key: true,
    requires_base_url: false,
    requires_model: false,
    detect_patterns: &["gpt"],
    model_discovery: crate::discovery::OPENAI_MODELS,
    model_discovery_for_model: None,
    create,
};

fn create(p: CreateParams) -> Result<Arc<dyn LlmProvider>> {
    let http_timeout = p.http_timeout();
    let key = p
        .api_key
        .ok_or_else(|| eyre::eyre!("OPENAI_API_KEY not set"))?;

    // Auto-detect: use Responses API for capable models when talking to OpenAI directly
    // (no custom base_url set, which would indicate a compatible provider).
    let is_openai_direct = p.base_url.is_none();

    // ChatGPT subscription OAuth tokens carry no `api.*` scopes — every
    // model call 403s on api.openai.com. The only endpoint that accepts
    // them is the Codex backend, and only for subscription-covered models.
    if is_openai_direct {
        if let Some(CredentialKind::ChatGptOAuth { account_id }) = &p.credential {
            // Default to gpt-5: the catalog default (gpt-4o) is a
            // platform-only model that a subscription cannot call.
            let model = p.model.clone().unwrap_or_else(|| "gpt-5".to_string());
            if !is_chatgpt_subscription_model(&model) {
                eyre::bail!(
                    "model '{model}' is not available with ChatGPT subscription login. \
                     Supported: gpt-5 family and codex models (gpt-5, gpt-5.1, gpt-5.1-codex, ...). \
                     For other models, use a platform API key from platform.openai.com \
                     (`octos auth logout -p openai`, then set OPENAI_API_KEY)."
                );
            }
            let mut provider =
                OpenAIResponsesProvider::new(&key, &model).with_chatgpt_oauth(account_id.clone());
            if let Some((t, c)) = http_timeout {
                provider = provider.with_http_timeout(t, c);
            }
            return Ok(Arc::new(provider));
        }
    }

    let model = p
        .model
        .or_else(|| ENTRY.default_model().map(str::to_string))
        .ok_or_else(|| {
            eyre::eyre!(
                "{}: no model given and the catalog declares no default for this family",
                ENTRY.name
            )
        })?;

    if is_openai_direct && is_responses_capable(&model) {
        let mut provider = OpenAIResponsesProvider::new(&key, &model);
        if let Some((t, c)) = http_timeout {
            provider = provider.with_http_timeout(t, c);
        }
        return Ok(Arc::new(provider));
    }

    let mut provider = OpenAIProvider::new(&key, &model);
    if let Some(url) = p.base_url {
        provider = provider.with_base_url(&url);
    }
    if let Some(hints) = p.model_hints {
        provider = provider.with_hints(hints);
    }
    if let Some((t, c)) = http_timeout {
        provider = provider.with_http_timeout(t, c);
    }
    Ok(Arc::new(provider))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::CredentialKind;

    fn params(model: Option<&str>, credential: Option<CredentialKind>) -> CreateParams {
        CreateParams {
            api_key: Some("tok".into()),
            model: model.map(str::to_string),
            base_url: None,
            model_hints: None,
            llm_timeout_secs: None,
            llm_connect_timeout_secs: None,
            credential,
        }
    }

    fn oauth(account_id: Option<&str>) -> Option<CredentialKind> {
        Some(CredentialKind::ChatGptOAuth {
            account_id: account_id.map(str::to_string),
        })
    }

    fn expect_reject(p: CreateParams) -> eyre::Report {
        match create(p) {
            Ok(_) => panic!("expected model rejection"),
            Err(e) => e,
        }
    }

    #[test]
    fn should_reject_platform_only_model_with_chatgpt_oauth() {
        let err = expect_reject(params(Some("gpt-4o"), oauth(None)));
        let msg = format!("{err:#}");
        assert!(msg.contains("gpt-4o"), "{msg}");
        assert!(msg.contains("subscription"), "{msg}");
        assert!(msg.contains("platform"), "{msg}");
    }

    #[test]
    fn should_reject_reasoning_only_model_with_chatgpt_oauth() {
        let err = expect_reject(params(Some("o3"), oauth(None)));
        assert!(format!("{err:#}").contains("o3"));
    }

    #[test]
    fn should_default_to_gpt5_with_chatgpt_oauth_when_no_model() {
        let provider = create(params(None, oauth(Some("acct")))).unwrap();
        assert_eq!(provider.model_id(), "gpt-5");
    }

    #[test]
    fn should_accept_subscription_model_with_chatgpt_oauth() {
        let provider = create(params(Some("gpt-5.1-codex"), oauth(None))).unwrap();
        assert_eq!(provider.model_id(), "gpt-5.1-codex");
    }

    #[test]
    fn should_keep_platform_models_with_api_key() {
        let provider = create(params(Some("gpt-4o"), None)).unwrap();
        assert_eq!(provider.model_id(), "gpt-4o");
    }

    #[test]
    fn should_respect_base_url_override_even_with_chatgpt_oauth() {
        // An explicit base_url means a user-managed proxy/compatible
        // endpoint — never force Codex backend routing there.
        let mut p = params(Some("gpt-4o"), oauth(None));
        p.base_url = Some("https://proxy.example.com/v1".into());
        let provider = create(p).unwrap();
        assert_eq!(provider.model_id(), "gpt-4o");
    }
}
