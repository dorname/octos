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
    let mut provider = AnthropicProvider::new(&key, &model).with_provider_label("minimax-token");
    if let Some(url) = p.base_url {
        provider = provider.with_base_url(&url);
    }
    if let Some((t, c)) = http_timeout {
        provider = provider.with_http_timeout(t, c);
    }
    Ok(Arc::new(provider))
}
