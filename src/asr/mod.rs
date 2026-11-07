pub mod phonon;
pub mod remote;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsrOutput {
    pub raw_text: String,
    pub latency_ms: f64,
    pub engine: String,
}

pub trait AsrEngine: Send + Sync {
    fn name(&self) -> &'static str;
    fn is_available(&self) -> bool;
    fn transcribe(&self, audio_path: &Path) -> impl std::future::Future<Output = Result<AsrOutput>> + Send;
}
