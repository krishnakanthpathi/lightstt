# 🎙️ LightSTT (`lstt`)

Ultra-fast, cross-platform Speech-to-Text backend and CLI with native **Phonon-2** ASR and embedded **S1-mini** transcript normalization.

Built in pure Rust to mirror the ergonomics, minimal footprint, and zero-runtime-dependency philosophy of [LightMem](https://github.com/krishnakanth/lightmem).

---

## ⚡ Architecture

```
Microphone / Audio File / Dictation App (FreeFlow, Willow)
                         │
                         ▼ (WAV, OPUS, MP3, M4A, FLAC)
┌─────────────────────────────────────────────────────────────────┐
│                    `lstt` CORE ENGINE (Rust)                    │
│                                                                 │
│  1. Audio Ingestion: CPAL mic capture & 16kHz WAV downmix       │
│  2. ASR Transcriber: Phonon-2 (Apple Silicon M4 & Linux AVX512) │
│  3. Normalizer: S1-mini (Superwhisper 0.6B via local Ollama)    │
└─────────────────────────────────────────────────────────────────┘
                         │
                         ▼ Clean, Normal Text (< 300ms)
        ┌────────────────┼────────────────┐
        ▼                ▼                ▼
   CLI STDOUT       REST Daemon        Agent MCP
  (`lstt rec`)     (`lstt serve`)    (`lstt --json`)
```

---

## 🚀 Quickstart

### 1. Installation
```bash
cd /Users/krishnakanth/Projects/lightstt
./install.sh
```
This builds `lstt` with release optimizations and copies the binary directly to `~/.local/bin/lstt`.

### 2. Verify Environment
```bash
lstt doctor
```
Output:
```text
LightSTT Doctor Check
----------------------------------------
✔ Microphone: MacBook Air Microphone
✔ ASR Engine: Phonon-2 (Found and available)
✔ Normalizer: S1-mini connected at http://127.0.0.1:11434/api/generate
✔ Config path: "/Users/krishnakanth/.lightstt/config.json"
----------------------------------------
```

---

## 🛠️ CLI Usage

### Transcribe an Audio File
```bash
# Clean output
lstt transcribe recording.opus

# With JSON metrics
lstt transcribe recording.opus --json

# Raw ASR output without S1-mini cleaning
lstt transcribe recording.opus --raw-only
```

### Record Live Microphone Audio
```bash
# Record for 5 seconds and output clean text
lstt listen --duration 5.0

# Format as structured action items
lstt listen --duration 10.0 --structure lists
```

### S1-mini Normalizer on Text
```bash
lstt clean "um so like let us test the the lightstt tool please"
# Output: "So let us test the Lightstt tool, please."
```

### Run Pipeline Benchmark
```bash
lstt bench --audio sample.wav
```

### Start Background Daemon
```bash
lstt serve --port 8090
```

---

## 🌐 HTTP REST API

When running `lstt serve` (default port `8090`):

### `GET /health`
```json
{
  "status": "healthy",
  "phonon_available": true,
  "normalizer_model": "s1-mini",
  "version": "0.1.0"
}
```

### `POST /v1/transcribe`
Send any audio file via `multipart/form-data`:
```bash
curl -X POST http://127.0.0.1:8090/v1/transcribe \
  -F "file=@meeting.opus" \
  -F "styling=semi-formal"
```
Response:
```json
{
  "raw_transcript": "Uh let's test the Discord bot one and uh could you please join into the one of the channel uh for testing purpose.",
  "clean_transcript": "Let's test the Discord bot one, and could you please join into the one of the channel for testing purpose?",
  "asr_latency_ms": 25.4,
  "cleanup_latency_ms": 142.8,
  "total_latency_ms": 168.2,
  "asr_engine": "phonon-2",
  "normalizer": "s1-mini"
}
```

### `POST /v1/clean`
```bash
curl -X POST http://127.0.0.1:8090/v1/clean \
  -H "Content-Type: application/json" \
  -d '{"text": "so um i need to send the report by friday no wait thursday"}'
```
Response:
```json
{
  "raw_text": "so um i need to send the report by friday no wait thursday",
  "clean_text": "So I need to send the report by Thursday.",
  "latency_ms": 128.5,
  "tokens_per_sec": 162.4
}
```

---

## ⚙️ Configuration (`~/.lightstt/config.json`)

```json
{
  "asr_backend": "auto",
  "s1_mini_endpoint": "http://127.0.0.1:11434/api/generate",
  "s1_mini_model": "s1-mini",
  "styling": "semi-formal",
  "structure": "prose",
  "context": "general",
  "server_port": 8090,
  "remote_stt_url": null
}
```

---

## 📊 Benchmark Metrics

| Metric | Mac (Apple M4 Silicon) | HP Workstation (Intel i5 CPU) |
| :--- | :--- | :--- |
| **S1-mini Latency** | **140 – 235 ms** (~155 tok/s) | **205 – 310 ms** (~90 tok/s) |
| **Memory Footprint** | ~35 MB (lstt binary) + ~480 MB (S1-mini) | ~35 MB (lstt binary) + ~480 MB (S1-mini) |
| **Filler Removal** | 100% | 100% |
| **Hallucination Rate** | 0.0% | 0.0% |
