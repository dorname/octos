use std::sync::Arc;

use eyre::Result;

use crate::anthropic::AnthropicProvider;
use crate::provider::LlmProvider;

use super::{CreateParams, ProviderEntry};

/// MiniMax **Token Plan** family — Anthropic Messages API compatible.
///
/// Distinct from the regular `minimax` family: this targets MiniMax's
/// Anthropic-compatible endpoint `https://api.minimaxi.com/anthropic` rather
/// than the OpenAI-compatible `https://api.minimax.io/v1`. Token Plan keys
/// are used here; the model catalog default is `MiniMax-M3`.
pub const ENTRY: ProviderEntry = ProviderEntry {
    name: "minimax-token",
    aliases: &["minimax-anthropic"],
    api_key_env: Some("MINIMAX_API_KEY"),
    key_env_aliases: &[],
    default_base_url: Some("https://api.minimaxi.com/anthropic"),
    requires_api_key: true,
    requires_base_url: false,
    requires_model: false,
    // Selected explicitly by family; do not auto-detect MiniMax model names
    // here because they should route to the native `minimax` family by default.
    detect_patterns: &[],
    model_discovery: crate::discovery::ANTHROPIC_MODELS,
    model_discovery_for_model: None,
    create,
};

fn create(p: CreateParams) -> Result<Arc<dyn LlmProvider>> {
    let http_timeout = p.http_timeout();
    let key = p
        .api_key
        .ok_or_else(|| eyre::eyre!("MINIMAX_API_KEY (MiniMax Token Plan) not set"))?;
    let model = p
        .model
        .or_else(|| ENTRY.default_model().map(str::to_string))
        .ok_or_else(|| {
            eyre::eyre!(
                "{}: no model given and the catalog declares no default for this family",
                ENTRY.name
            )
        })?;
    // Mirror zai / zai_coding: always pin the Token Plan Anthropic root when
    // the caller omits base_url. Without this, `AnthropicProvider::new`
    // silently targets api.anthropic.com and MiniMax keys 401 with
    // "Please carry the API secret key in the 'X-Api-Key' field".
    let url = p
        .base_url
        .unwrap_or_else(|| ENTRY.default_base_url.expect("ENTRY declares default").into());
    let mut provider = AnthropicProvider::new(&key, &model)
        .with_provider_label("minimax-token")
        .with_base_url(&url);
    if let Some((t, c)) = http_timeout {
        provider = provider.with_http_timeout(t, c);
    }
    Ok(Arc::new(provider))
}

#[cfg(test)]
mod tests {
    use octos_core::Message;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::config::ChatConfig;

    #[tokio::test]
    async fn should_target_minimax_anthropic_root_when_base_url_omitted() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "mm-token-key"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(
                        r#"{"content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}}"#,
                    )
                    .append_header("Content-Type", "application/json"),
            )
            .mount(&server)
            .await;

        // Override only for the probe — production create uses ENTRY.default
        // when base_url is None; here we point at the mock while still
        // asserting the x-api-key header MiniMax requires.
        let provider = create(CreateParams {
            api_key: Some("mm-token-key".into()),
            model: Some("MiniMax-M3".into()),
            base_url: Some(server.uri()),
            model_hints: None,
            llm_timeout_secs: None,
            llm_connect_timeout_secs: None,
        })
        .unwrap();
        assert_eq!(provider.provider_name(), "minimax-token");
        provider
            .chat(&[Message::user("hi")], &[], &ChatConfig::default())
            .await
            .unwrap();
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[test]
    fn should_apply_default_base_url_when_create_omits_override() {
        let provider = create(CreateParams {
            api_key: Some("mm-token-key".into()),
            model: Some("MiniMax-M3".into()),
            base_url: None,
            model_hints: None,
            llm_timeout_secs: None,
            llm_connect_timeout_secs: None,
        })
        .expect("create without base_url must succeed");
        let meta = provider.provider_metadata();
        assert_eq!(provider.provider_name(), "minimax-token");
        assert!(
            meta.endpoint
                .as_deref()
                .is_some_and(|e| e.contains("minimaxi.com") || e.contains("minimax")),
            "omitted base_url must pin the MiniMax Token Plan Anthropic root, got {:?}",
            meta.endpoint
        );
    }
}
