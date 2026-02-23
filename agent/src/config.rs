use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub database: DatabaseConfig,
    pub capture:  CaptureConfig,
    #[serde(default)]
    pub kg:       KgConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub url: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CaptureConfig {
    /// How many seconds between screen captures
    pub interval_secs: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct KgConfig {
    /// Ollama endpoint for local LLM NER. Leave empty to disable LLM extraction.
    /// Example: "http://localhost:11434"
    #[serde(default = "default_ollama_endpoint")]
    pub ollama_endpoint: String,

    /// Ollama model name to use for NER.
    #[serde(default = "default_ollama_model")]
    pub ollama_model: String,

    /// Anthropic API key — used only by the cortex-query binary for answer synthesis.
    #[serde(default)]
    pub anthropic_api_key: String,

    /// Gap (in minutes) between frames that triggers a new session boundary.
    #[serde(default = "default_session_gap_mins")]
    pub session_gap_mins: u32,

    /// Number of days after which raw OCR text is expired (set to NULL).
    #[serde(default = "default_ocr_ttl_days")]
    pub ocr_ttl_days: u32,

    /// Fraction of frames to send to LLM (0.0–1.0).
    /// 1.0 = process every frame; 0.5 = process every other frame (~50% cost reduction).
    #[serde(default = "default_llm_sample_rate")]
    pub llm_sample_rate: f64,
}

fn default_ollama_endpoint() -> String { "http://localhost:11434".to_string() }
fn default_ollama_model()    -> String { "deepseek-r1:7b".to_string() }
fn default_session_gap_mins() -> u32   { 30 }
fn default_ocr_ttl_days()     -> u32   { 14 }
fn default_llm_sample_rate()  -> f64   { 1.0 }

impl Default for KgConfig {
    fn default() -> Self {
        Self {
            ollama_endpoint:   default_ollama_endpoint(),
            ollama_model:      default_ollama_model(),
            anthropic_api_key: String::new(),
            session_gap_mins:  default_session_gap_mins(),
            ocr_ttl_days:      default_ocr_ttl_days(),
            llm_sample_rate:   default_llm_sample_rate(),
        }
    }
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let content = fs::read_to_string(path.as_ref())
            .with_context(|| format!("Failed to read config file: {}", path.as_ref().display()))?;
        let config: Config = toml::from_str(&content).context("Failed to parse config.toml")?;
        Ok(config)
    }
}
