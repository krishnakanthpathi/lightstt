use super::{AsrEngine, AsrOutput};
use anyhow::{anyhow, Context, Result};
use reqwest::multipart::{Form, Part};
use std::path::Path;
use std::time::Instant;
use tokio::fs;

#[derive(Debug, Clone)]
pub struct RemoteEngine {
    pub endpoint_url: String,
    pub model_name: String,
}

impl RemoteEngine {
    pub fn new(endpoint_url: String, model_name: Option<String>) -> Self {
        Self {
            endpoint_url,
            model_name: model_name.unwrap_or_else(|| "default".to_string()),
        }
    }
}

impl AsrEngine for RemoteEngine {
    fn name(&self) -> &'static str {
        "Remote HTTP STT"
    }

    fn is_available(&self) -> bool {
        !self.endpoint_url.is_empty()
    }

    async fn transcribe(&self, audio_path: &Path) -> Result<AsrOutput> {
        if !audio_path.exists() {
            return Err(anyhow!("Audio file does not exist: {:?}", audio_path));
        }

        let start = Instant::now();
        let file_bytes = fs::read(audio_path)
            .await
            .with_context(|| format!("Failed to read audio file {:?}", audio_path))?;
        
        let file_name = audio_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("audio.wav")
            .to_string();

        let part = Part::bytes(file_bytes)
            .file_name(file_name)
            .mime_str("audio/wav")?;

        let form = Form::new()
            .part("file", part)
            .text("model", self.model_name.clone());

        let client = reqwest::Client::new();
        let resp = client
            .post(&self.endpoint_url)
            .multipart(form)
            .send()
            .await
            .with_context(|| format!("Failed to connect to STT endpoint {}", self.endpoint_url))?;

        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;

        let status = resp.status();
        if !status.is_success() {
            let error_text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Remote STT error (status {}): {}", status, error_text));
        }

        let body: serde_json::Value = resp.json().await?;
        let raw_text = body["text"]
            .as_str()
            .or_else(|| body["transcription"]["text"].as_str())
            .unwrap_or("")
            .trim()
            .to_string();

        Ok(AsrOutput {
            raw_text,
            latency_ms,
            engine: format!("remote:{}", self.endpoint_url),
        })
    }
}
