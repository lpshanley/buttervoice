use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::llm_cleanup::{chat_completion, ChatCompletionParams, LlmCleanupError};
use crate::settings::Settings;

pub const DEFAULT_CLASSIFICATION_PROMPT: &str = r#"
You are a text content classifier. Analyze the following text and return a JSON object with your classification.

Categories to evaluate:
- profanity: explicit language, swear words, vulgar expressions
- toxicity: hostile, aggressive, or harmful language
- unprofessional: informal, sloppy, or inappropriate for workplace communication
- sensitive: references to sensitive topics (politics, religion, personal health)

Return ONLY valid JSON in this exact format, no other text:
{"score": <float 0.0-1.0, overall severity>, "categories": [{"tag": "<category>", "score": <float 0.0-1.0>, "severity": "<low|medium|high>"}]}

Only include categories that have a non-zero score. If the text is clean, return score 0.0 with empty categories.
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentClassificationResult {
    pub score: f32,
    pub categories: Vec<ClassificationTag>,
    #[serde(default)]
    pub blocked: bool,
    #[serde(default)]
    pub warning: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationTag {
    pub tag: String,
    pub score: f32,
    pub severity: String,
}

/// Raw JSON shape returned by the LLM (before we add blocked/warning fields).
#[derive(Debug, Deserialize)]
struct ClassificationRaw {
    #[serde(default)]
    score: f32,
    #[serde(default)]
    categories: Vec<ClassificationTag>,
}

pub fn classify_text<F>(
    settings: &Settings,
    text: &str,
    timeout: Duration,
    trace: F,
) -> Result<ContentClassificationResult, LlmCleanupError>
where
    F: FnMut(&str),
{
    let model = resolve_classification_model(settings);

    let system_prompt = if settings.content_classification_use_custom_prompt
        && !settings
            .content_classification_custom_prompt
            .trim()
            .is_empty()
    {
        settings.content_classification_custom_prompt.as_str()
    } else {
        DEFAULT_CLASSIFICATION_PROMPT
    };

    let params = ChatCompletionParams {
        base_url: &settings.llm_cleanup_base_url,
        api_key: &settings.llm_cleanup_api_key,
        model,
        system_prompt,
        user_message: text,
        temperature: 0.0,
        timeout,
        debug_logging: settings.debug_logging,
        debug_log_include_content: settings.debug_log_include_content,
    };

    let response = chat_completion(&params, trace)?;

    // Strip potential markdown code fences around JSON
    let json_str = response.trim();
    let json_str = if json_str.starts_with("```") {
        let stripped = json_str
            .strip_prefix("```json")
            .or_else(|| json_str.strip_prefix("```"))
            .unwrap_or(json_str);
        stripped.strip_suffix("```").unwrap_or(stripped).trim()
    } else {
        json_str
    };

    let raw: ClassificationRaw = serde_json::from_str(json_str).map_err(|e| {
        LlmCleanupError::bad_response(format!(
            "failed parsing classification JSON: {e}. Response was: {json_str}"
        ))
    })?;

    let score = raw.score.clamp(0.0, 1.0);
    let warning = score >= settings.content_classification_warning_threshold;
    let blocked = score >= settings.content_classification_block_threshold;

    Ok(ContentClassificationResult {
        score,
        categories: raw.categories,
        blocked,
        warning,
    })
}

pub fn default_classification_prompt() -> &'static str {
    DEFAULT_CLASSIFICATION_PROMPT
}

fn resolve_classification_model(settings: &Settings) -> &str {
    let override_model = settings.content_classification_model_override.trim();
    if !override_model.is_empty() {
        override_model
    } else {
        settings.llm_cleanup_model.trim()
    }
}
