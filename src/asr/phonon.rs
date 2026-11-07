use super::{AsrEngine, AsrOutput};
use anyhow::{anyhow, Context, Result};
use dirs::home_dir;
use reqwest::multipart::{Form, Part};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::fs;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct PhononEngine {
    binary_path: Option<PathBuf>,
    daemon_url: String,
}

impl Default for PhononEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PhononEngine {
    pub fn new() -> Self {
        Self {
            binary_path: Self::discover_binary(),
            daemon_url: "http://127.0.0.1:8000/v1/audio/transcriptions".to_string(),
        }
    }

    fn discover_binary() -> Option<PathBuf> {
        // 1. Try PATH lookup
        if let Ok(output) = std::process::Command::new("which").arg("phonon").output() {
            if output.status.success() {
                let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path_str.is_empty() {
                    return Some(PathBuf::from(path_str));
                }
            }
        }

        // 2. Check well-known fallback paths
        let home = home_dir();
        let mut candidates = vec![
            PathBuf::from("/Library/Frameworks/Python.framework/Versions/3.14/bin/phonon"),
            PathBuf::from("/opt/homebrew/bin/phonon"),
            PathBuf::from("/usr/local/bin/phonon"),
            PathBuf::from("/usr/bin/phonon"),
        ];

        if let Some(h) = home {
            candidates.push(h.join(".local/bin/phonon"));
            candidates.push(h.join("phonon_env/bin/phonon"));
            candidates.push(h.join(".cargo/bin/phonon"));
        }

        for p in candidates {
            if p.exists() {
                return Some(p);
            }
        }

        None
    }

    async fn try_daemon_transcribe(&self, audio_path: &Path) -> Result<AsrOutput> {
        let start = Instant::now();
        let file_bytes = fs::read(audio_path).await?;
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
            .text("model", "phonon-2");

        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(4000))
            .build()?;

        let resp = client
            .post(&self.daemon_url)
            .multipart(form)
            .send()
            .await?;

        if !resp.status().is_success() {
            let err = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Phonon daemon returned error: {}", err));
        }

        let body: serde_json::Value = resp.json().await?;
        let raw_text = body["text"].as_str().unwrap_or("").trim().to_string();
        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;

        Ok(AsrOutput {
            raw_text,
            latency_ms,
            engine: "phonon-2 (daemon warm)".to_string(),
        })
    }
}

impl AsrEngine for PhononEngine {
    fn name(&self) -> &'static str {
        "Phonon-2 (Fermion Parakeet TDT)"
    }

    fn is_available(&self) -> bool {
        self.binary_path.is_some()
    }

    async fn transcribe(&self, audio_path: &Path) -> Result<AsrOutput> {
        if !audio_path.exists() {
            return Err(anyhow!("Audio file not found: {:?}", audio_path));
        }

        // Fast Path 1: Check if warm daemon is listening on port 8000
        if let Ok(warm_output) = self.try_daemon_transcribe(audio_path).await {
            return Ok(warm_output);
        }

        // Slow Path 2: Fall back to one-shot CLI process
        let binary = self
            .binary_path
            .as_ref()
            .ok_or_else(|| anyhow!("Phonon CLI not found. Install with: pip install fermion-research"))?;

        let start = Instant::now();

        let output = Command::new(binary)
            .arg("transcribe")
            .arg(audio_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .with_context(|| format!("Failed to execute {:?} on {:?}", binary, audio_path))?;

        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Phonon transcribe failed: {}", stderr.trim()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let raw_text = stdout
            .lines()
            .filter(|line| !line.starts_with("[fermion]"))
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();

        Ok(AsrOutput {
            raw_text,
            latency_ms,
            engine: "phonon-2 (cli one-shot)".to_string(),
        })
    }
}
