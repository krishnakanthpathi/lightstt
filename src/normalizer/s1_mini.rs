use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizerOptions {
    pub styling: String,
    pub structure: String,
    pub context: String,
}

impl Default for NormalizerOptions {
    fn default() -> Self {
        Self {
            styling: "semi-formal".to_string(),
            structure: "prose".to_string(),
            context: "general".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedOutput {
    pub clean_text: String,
    pub latency_ms: f64,
    pub tokens_per_sec: f64,
}

#[derive(Debug, Clone)]
pub struct S1MiniNormalizer {
    pub endpoint: String,
    pub model: String,
    client: Client,
}

impl Default for S1MiniNormalizer {
    fn default() -> Self {
        Self::new("http://127.0.0.1:11434/api/generate", "s1-mini")
    }
}

impl S1MiniNormalizer {
    pub fn new(endpoint: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            model: model.into(),
            client: Client::new(),
        }
    }

    pub async fn normalize(&self, raw_text: &str, opts: &NormalizerOptions) -> Result<NormalizedOutput> {
        let trimmed = raw_text.trim();
        if trimmed.is_empty() {
            return Ok(NormalizedOutput {
                clean_text: String::new(),
                latency_ms: 0.0,
                tokens_per_sec: 0.0,
            });
        }

        let start = Instant::now();

        // Exact training prefix matching Qwen3-0.6B non-thinking format
        let prompt = format!(
            "<|im_start|>system\n\
            You are a text normalizer for speech-to-text transcripts. The input begins with a control line specifying the styling, structure, and context settings; clean the transcript to match those settings and output only the cleaned text.<|im_end|>\n\
            <|im_start|>user\n\
            [Styling: {}] [Structure: {}] [Context: {}]\n\
            {}<|im_end|>\n\
            <|im_start|>assistant\n\
            <think>\n\n</think>\n\n",
            opts.styling, opts.structure, opts.context, trimmed
        );

        let body = json!({
            "model": self.model,
            "prompt": prompt,
            "raw": true,
            "stream": false,
            "options": {
                "temperature": 0
            }
        });

        let resp = self
            .client
            .post(&self.endpoint)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("Failed to connect to S1-mini endpoint at {}", self.endpoint))?;

        let total_ms = start.elapsed().as_secs_f64() * 1000.0;

        let res_json: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse S1-mini JSON response")?;

        let clean_text = res_json["response"]
            .as_str()
            .unwrap_or("")
            .trim()
            .to_string();

        let eval_count = res_json["eval_count"].as_f64().unwrap_or(0.0);
        let eval_duration_ns = res_json["eval_duration"].as_f64().unwrap_or(1.0);
        let tokens_per_sec = if eval_duration_ns > 0.0 {
            eval_count / (eval_duration_ns / 1_000_000_000.0)
        } else {
            0.0
        };

        Ok(NormalizedOutput {
            clean_text,
            latency_ms: total_ms,
            tokens_per_sec,
        })
    }
}
