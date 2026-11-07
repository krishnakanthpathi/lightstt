use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use hound::{WavSpec, WavWriter};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::sleep;

pub struct AudioRecorder;

impl AudioRecorder {
    pub fn default_input_device_name() -> Result<String> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow!("No default audio input device found"))?;
        device
            .name()
            .context("Failed to get audio device name")
    }

    pub async fn record(output_path: &Path, duration: Duration) -> Result<PathBuf> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow!("No audio input device available"))?;

        let supported_config = device
            .default_input_config()
            .context("Failed to get default input config")?;

        let sample_rate = supported_config.sample_rate().0;
        let channels = supported_config.channels();

        let samples_buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
        let samples_clone = Arc::clone(&samples_buffer);

        let err_fn = |err| eprintln!("Audio stream error: {}", err);

        let stream = match supported_config.sample_format() {
            SampleFormat::F32 => device.build_input_stream(
                &supported_config.into(),
                move |data: &[f32], _: &_| {
                    if let Ok(mut buf) = samples_clone.lock() {
                        buf.extend_from_slice(data);
                    }
                },
                err_fn,
                None,
            )?,
            SampleFormat::I16 => {
                let samples_clone = Arc::clone(&samples_buffer);
                device.build_input_stream(
                    &supported_config.into(),
                    move |data: &[i16], _: &_| {
                        if let Ok(mut buf) = samples_clone.lock() {
                            for &s in data {
                                buf.push(s as f32 / i16::MAX as f32);
                            }
                        }
                    },
                    err_fn,
                    None,
                )?
            }
            sample_format => return Err(anyhow!("Unsupported sample format: {:?}", sample_format)),
        };

        stream.play().context("Failed to start audio input stream")?;

        // Sleep for requested duration
        sleep(duration).await;

        drop(stream);

        // Extract captured samples and convert/downmix to 16kHz mono WAV for ASR
        let raw_samples = samples_buffer
            .lock()
            .map_err(|_| anyhow!("Failed to lock sample buffer"))?
            .clone();

        if raw_samples.is_empty() {
            return Err(anyhow!("No audio samples captured from microphone"));
        }

        // Downmix to mono if multi-channel
        let mono_samples: Vec<f32> = if channels > 1 {
            raw_samples
                .chunks(channels as usize)
                .map(|ch| ch.iter().sum::<f32>() / channels as f32)
                .collect()
        } else {
            raw_samples
        };

        // Write standard 16kHz mono WAV (hound)
        let spec = WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut writer = WavWriter::create(output_path, spec)
            .with_context(|| format!("Failed to create WAV file at {:?}", output_path))?;

        for sample in mono_samples {
            let clamped = sample.clamp(-1.0, 1.0);
            let int_sample = (clamped * i16::MAX as f32) as i16;
            writer.write_sample(int_sample)?;
        }

        writer.finalize()?;

        Ok(output_path.to_path_buf())
    }
}

pub async fn record_to_wav(duration: Duration) -> Result<PathBuf> {
    let tmp_dir = std::env::temp_dir();
    let filename = format!("lstt_rec_{}.wav", chrono::Utc::now().timestamp_millis());
    let path = tmp_dir.join(filename);
    AudioRecorder::record(&path, duration).await
}
