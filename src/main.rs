use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use colored::*;
use lightstt::asr::phonon::PhononEngine;
use lightstt::asr::remote::RemoteEngine;
use lightstt::asr::{AsrEngine, AsrOutput};
use lightstt::audio::capture::{record_to_wav, AudioRecorder};
use lightstt::config::{AsrBackendType, Config};
use lightstt::normalizer::s1_mini::{NormalizerOptions, S1MiniNormalizer};
use lightstt::server::run_server;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Parser)]
#[command(name = "lstt", author, version, about = "Ultra-fast cross-platform Speech-to-Text CLI and daemon with S1-mini normalization")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Transcribe an audio file (WAV, OPUS, MP3, M4A, FLAC) and clean it
    Transcribe {
        /// Path to audio file
        file: PathBuf,

        /// Output full JSON metrics breakdown
        #[arg(long)]
        json: bool,

        /// Styling register (casual, semi-casual, semi-formal, formal)
        #[arg(long, default_value = "semi-formal")]
        styling: String,

        /// Output structure (prose, lists)
        #[arg(long, default_value = "prose")]
        structure: String,

        /// Context domain (general, email)
        #[arg(long, default_value = "general")]
        context: String,

        /// Return raw uncleaned ASR transcript without running S1-mini
        #[arg(long)]
        raw_only: bool,
    },

    /// Record from microphone and transcribe in real-time
    Listen {
        /// Recording duration in seconds
        #[arg(short, long, default_value_t = 5.0)]
        duration: f64,

        /// Output full JSON metrics breakdown
        #[arg(long)]
        json: bool,

        /// Styling register
        #[arg(long, default_value = "semi-formal")]
        styling: String,

        /// Output structure
        #[arg(long, default_value = "prose")]
        structure: String,

        /// Context domain
        #[arg(long, default_value = "general")]
        context: String,
    },

    /// Run S1-mini transcript normalizer directly on text
    Clean {
        /// Raw transcript text to normalize
        text: String,

        /// Styling register
        #[arg(long, default_value = "semi-formal")]
        styling: String,

        /// Output structure
        #[arg(long, default_value = "prose")]
        structure: String,

        /// Context domain
        #[arg(long, default_value = "general")]
        context: String,

        /// Output JSON with latency metrics
        #[arg(long)]
        json: bool,
    },

    /// Start the LightSTT local HTTP daemon
    Serve {
        /// Port to bind the server to (overrides config)
        #[arg(short, long)]
        port: Option<u16>,
    },

    /// Run benchmark on an audio file or sample
    Bench {
        /// Audio file to benchmark
        #[arg(short, long)]
        audio: PathBuf,
    },

    /// Verify local environment, ASR engine, Ollama S1-mini, and audio inputs
    Doctor,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut config = Config::load().unwrap_or_default();

    match cli.command {
        Commands::Transcribe {
            file,
            json,
            styling,
            structure,
            context,
            raw_only,
        } => {
            let asr_output = run_asr(&config, &file).await?;

            if raw_only {
                if json {
                    println!("{}", serde_json::to_string_pretty(&asr_output)?);
                } else {
                    println!("{}", asr_output.raw_text);
                }
                return Ok(());
            }

            let normalizer = S1MiniNormalizer::new(&config.s1_mini_endpoint, &config.s1_mini_model);
            let opts = NormalizerOptions {
                styling,
                structure,
                context,
            };

            let norm_output = normalizer.normalize(&asr_output.raw_text, &opts).await?;
            let total_ms = asr_output.latency_ms + norm_output.latency_ms;

            if json {
                let res = json!({
                    "raw_transcript": asr_output.raw_text,
                    "clean_transcript": norm_output.clean_text,
                    "asr_latency_ms": asr_output.latency_ms,
                    "cleanup_latency_ms": norm_output.latency_ms,
                    "total_latency_ms": total_ms,
                    "asr_engine": asr_output.engine,
                    "normalizer": "s1-mini",
                    "normalizer_tok_per_sec": norm_output.tokens_per_sec,
                });
                println!("{}", serde_json::to_string_pretty(&res)?);
            } else {
                println!("{}", norm_output.clean_text);
            }
        }

        Commands::Listen {
            duration,
            json,
            styling,
            structure,
            context,
        } => {
            let dur = Duration::from_secs_f64(duration);
            eprintln!("🎙️  {} ({}s)...", "Listening".cyan().bold(), duration);

            let audio_path = record_to_wav(dur).await?;
            let asr_output = run_asr(&config, &audio_path).await?;

            let normalizer = S1MiniNormalizer::new(&config.s1_mini_endpoint, &config.s1_mini_model);
            let opts = NormalizerOptions {
                styling,
                structure,
                context,
            };

            let norm_output = normalizer.normalize(&asr_output.raw_text, &opts).await?;
            let total_ms = asr_output.latency_ms + norm_output.latency_ms;

            // Remove temporary recording
            let _ = tokio::fs::remove_file(&audio_path).await;

            if json {
                let res = json!({
                    "raw_transcript": asr_output.raw_text,
                    "clean_transcript": norm_output.clean_text,
                    "asr_latency_ms": asr_output.latency_ms,
                    "cleanup_latency_ms": norm_output.latency_ms,
                    "total_latency_ms": total_ms,
                    "asr_engine": asr_output.engine,
                    "normalizer": "s1-mini",
                });
                println!("{}", serde_json::to_string_pretty(&res)?);
            } else {
                println!("{}", norm_output.clean_text);
            }
        }

        Commands::Clean {
            text,
            styling,
            structure,
            context,
            json,
        } => {
            let normalizer = S1MiniNormalizer::new(&config.s1_mini_endpoint, &config.s1_mini_model);
            let opts = NormalizerOptions {
                styling,
                structure,
                context,
            };

            let out = normalizer.normalize(&text, &opts).await?;

            if json {
                let res = json!({
                    "raw_text": text,
                    "clean_text": out.clean_text,
                    "latency_ms": out.latency_ms,
                    "tokens_per_sec": out.tokens_per_sec,
                });
                println!("{}", serde_json::to_string_pretty(&res)?);
            } else {
                println!("{}", out.clean_text);
            }
        }

        Commands::Serve { port } => {
            if let Some(p) = port {
                config.server_port = p;
            }
            run_server(config).await?;
        }

        Commands::Bench { audio } => {
            if !audio.exists() {
                return Err(anyhow!("Audio file not found at {:?}", audio));
            }

            println!("{}", "═══════════════════════════════════════════════════".dimmed());
            println!("  {}", "LightSTT (lstt) End-to-End Pipeline Benchmark".bold().cyan());
            println!("{}", "═══════════════════════════════════════════════════".dimmed());
            println!("Audio File: {:?}", audio);

            let asr_out = run_asr(&config, &audio).await?;
            println!("\n[{}] ({:.1} ms)", "Stage 1: ASR".bold().green(), asr_out.latency_ms);
            println!("Engine:  {}", asr_out.engine.yellow());
            println!("Raw STT: {}", asr_out.raw_text);

            let normalizer = S1MiniNormalizer::new(&config.s1_mini_endpoint, &config.s1_mini_model);
            let opts = NormalizerOptions::default();
            let norm_out = normalizer.normalize(&asr_out.raw_text, &opts).await?;

            println!("\n[{}] ({:.1} ms)", "Stage 2: S1-mini Cleanup".bold().green(), norm_out.latency_ms);
            println!("Clean:   {}", norm_out.clean_text.cyan().bold());
            println!("Speed:   {:.1} tokens/sec", norm_out.tokens_per_sec);

            println!("\n{}", "───────────────────────────────────────────────────".dimmed());
            let total = asr_out.latency_ms + norm_out.latency_ms;
            println!("Total Pipeline Latency: {}", format!("{:.1} ms", total).bold().green());
            println!("{}", "═══════════════════════════════════════════════════".dimmed());
        }

        Commands::Doctor => {
            println!("{}", "LightSTT Doctor Check".bold().cyan());
            println!("{}", "----------------------------------------".dimmed());

            // 1. Microphone
            match AudioRecorder::default_input_device_name() {
                Ok(name) => println!("{} Microphone: {}", "✔".green(), name),
                Err(e) => println!("{} Microphone: {}", "✖".red(), e),
            }

            // 2. Phonon-2
            let phonon = PhononEngine::new();
            if phonon.is_available() {
                println!("{} ASR Engine: Phonon-2 (Found and available)", "✔".green());
            } else {
                println!("{} ASR Engine: Phonon-2 not found in PATH", "✖".red());
            }

            // 3. Ollama S1-mini
            let norm = S1MiniNormalizer::new(&config.s1_mini_endpoint, &config.s1_mini_model);
            match norm.normalize("hello world test", &NormalizerOptions::default()).await {
                Ok(_) => println!("{} Normalizer: S1-mini connected at {}", "✔".green(), config.s1_mini_endpoint),
                Err(e) => println!("{} Normalizer: S1-mini error ({})", "✖".red(), e),
            }

            // 4. Config
            println!("{} Config path: {:?}", "✔".green(), Config::config_path());
            println!("{}", "----------------------------------------".dimmed());
        }
    }

    Ok(())
}

async fn run_asr(config: &Config, file_path: &Path) -> Result<AsrOutput> {
    let phonon = PhononEngine::new();
    let remote = config
        .remote_stt_url
        .as_ref()
        .map(|url| RemoteEngine::new(url.clone(), None));

    match config.asr_backend {
        AsrBackendType::Phonon => phonon.transcribe(file_path).await,
        AsrBackendType::Remote => {
            let r = remote.ok_or_else(|| anyhow!("remote_stt_url is not configured"))?;
            r.transcribe(file_path).await
        }
        AsrBackendType::Auto => {
            if phonon.is_available() {
                phonon.transcribe(file_path).await
            } else if let Some(r) = remote {
                r.transcribe(file_path).await
            } else {
                Err(anyhow!(
                    "No ASR engine available. Please install Phonon (`pip install fermion-research`) or configure remote_stt_url"
                ))
            }
        }
    }
}
