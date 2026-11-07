use crate::asr::phonon::PhononEngine;
use crate::asr::remote::RemoteEngine;
use crate::asr::AsrEngine;
use crate::config::{AsrBackendType, Config};
use crate::normalizer::s1_mini::{NormalizerOptions, S1MiniNormalizer};
use anyhow::Result;
use axum::extract::Multipart;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tower_http::cors::CorsLayer;
use tracing::info;

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub phonon: PhononEngine,
    pub remote: Option<RemoteEngine>,
    pub normalizer: S1MiniNormalizer,
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub phonon_available: bool,
    pub normalizer_model: String,
    pub version: &'static str,
}

#[derive(Deserialize)]
pub struct CleanRequest {
    pub text: String,
    pub styling: Option<String>,
    pub structure: Option<String>,
    pub context: Option<String>,
}

#[derive(Serialize)]
pub struct CleanResponse {
    pub raw_text: String,
    pub clean_text: String,
    pub latency_ms: f64,
    pub tokens_per_sec: f64,
}

#[derive(Serialize)]
pub struct TranscribeResponse {
    pub raw_transcript: String,
    pub clean_transcript: String,
    pub asr_latency_ms: f64,
    pub cleanup_latency_ms: f64,
    pub total_latency_ms: f64,
    pub asr_engine: String,
    pub normalizer: &'static str,
}

pub async fn run_server(config: Config) -> Result<()> {
    let port = config.server_port;
    let phonon = PhononEngine::new();
    let remote = config
        .remote_stt_url
        .as_ref()
        .map(|url| RemoteEngine::new(url.clone(), None));
    let normalizer = S1MiniNormalizer::new(&config.s1_mini_endpoint, &config.s1_mini_model);

    let state = Arc::new(AppState {
        config,
        phonon,
        remote,
        normalizer,
    });

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/v1/clean", post(clean_handler))
        .route("/v1/transcribe", post(transcribe_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("LightSTT server listening on http://{}", addr);
    println!("🚀 LightSTT server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> impl IntoResponse {
    Json(HealthResponse {
        status: "healthy",
        phonon_available: state.phonon.is_available(),
        normalizer_model: state.config.s1_mini_model.clone(),
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn clean_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<CleanRequest>,
) -> Result<Json<CleanResponse>, (StatusCode, String)> {
    let opts = NormalizerOptions {
        styling: req.styling.unwrap_or_else(|| state.config.styling.clone()),
        structure: req.structure.unwrap_or_else(|| state.config.structure.clone()),
        context: req.context.unwrap_or_else(|| state.config.context.clone()),
    };

    let norm_res = state
        .normalizer
        .normalize(&req.text, &opts)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(CleanResponse {
        raw_text: req.text,
        clean_text: norm_res.clean_text,
        latency_ms: norm_res.latency_ms,
        tokens_per_sec: norm_res.tokens_per_sec,
    }))
}

async fn transcribe_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<TranscribeResponse>, (StatusCode, String)> {
    let mut temp_path: Option<PathBuf> = None;
    let mut styling = state.config.styling.clone();
    let mut structure = state.config.structure.clone();
    let mut context = state.config.context.clone();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
    {
        let name = field.name().unwrap_or_default().to_string();
        if name == "file" {
            let file_name = field.file_name().unwrap_or("upload.wav").to_string();
            let data = field
                .bytes()
                .await
                .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

            let tmp = std::env::temp_dir().join(format!(
                "lstt_upload_{}_{}",
                chrono::Utc::now().timestamp_millis(),
                file_name
            ));
            fs::write(&tmp, &data)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            temp_path = Some(tmp);
        } else if name == "styling" {
            if let Ok(text) = field.text().await {
                styling = text;
            }
        } else if name == "structure" {
            if let Ok(text) = field.text().await {
                structure = text;
            }
        } else if name == "context" {
            if let Ok(text) = field.text().await {
                context = text;
            }
        }
    }

    let file_path = temp_path.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            "Missing 'file' field in multipart payload".to_string(),
        )
    })?;

    // Execute ASR
    let asr_output = match state.config.asr_backend {
        AsrBackendType::Phonon => state
            .phonon
            .transcribe(&file_path)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?,
        AsrBackendType::Remote => {
            let remote = state.remote.as_ref().ok_or_else(|| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Remote STT backend configured but no remote_stt_url provided".to_string(),
                )
            })?;
            remote
                .transcribe(&file_path)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        }
        AsrBackendType::Auto => {
            if state.phonon.is_available() {
                state
                    .phonon
                    .transcribe(&file_path)
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            } else if let Some(ref remote) = state.remote {
                remote
                    .transcribe(&file_path)
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            } else {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "No ASR engine available (Phonon not installed, no remote STT configured)"
                        .to_string(),
                ));
            }
        }
    };

    // Clean up temporary audio file
    let _ = fs::remove_file(&file_path).await;

    // Execute Normalization
    let norm_opts = NormalizerOptions {
        styling,
        structure,
        context,
    };
    let norm_output = state
        .normalizer
        .normalize(&asr_output.raw_text, &norm_opts)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(TranscribeResponse {
        raw_transcript: asr_output.raw_text,
        clean_transcript: norm_output.clean_text,
        asr_latency_ms: asr_output.latency_ms,
        cleanup_latency_ms: norm_output.latency_ms,
        total_latency_ms: asr_output.latency_ms + norm_output.latency_ms,
        asr_engine: asr_output.engine,
        normalizer: "s1-mini",
    }))
}
