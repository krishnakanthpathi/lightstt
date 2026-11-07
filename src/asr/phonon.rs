use super::{AsrEngine, AsrOutput};
use anyhow::{anyhow, Context, Result};
use dirs::home_dir;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Instant;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct PhononEngine {
    binary_path: Option<PathBuf>,
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
}

impl AsrEngine for PhononEngine {
    fn name(&self) -> &'static str {
        "Phonon-2 (Fermion Parakeet TDT)"
    }

    fn is_available(&self) -> bool {
        self.binary_path.is_some()
    }

    async fn transcribe(&self, audio_path: &Path) -> Result<AsrOutput> {
        let binary = self
            .binary_path
            .as_ref()
            .ok_or_else(|| anyhow!("Phonon CLI not found. Install with: pip install fermion-research"))?;

        if !audio_path.exists() {
            return Err(anyhow!("Audio file not found: {:?}", audio_path));
        }

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
            engine: "phonon-2".to_string(),
        })
    }
}
