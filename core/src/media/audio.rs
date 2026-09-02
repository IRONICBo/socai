use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::media::asr::transcribe_local_file;
use crate::media::common::{url_suffix, MediaUnavailable};
use crate::media::processor::MediaProcessor;

impl MediaProcessor {
    /// Transcribe a video/audio source with the locally installed
    /// Qwen3-ASR 0.6B Int8 model. Inference never calls an ASR service.
    pub async fn transcribe_audio(&self, source: &str, referer: &str) -> Result<String> {
        let t0 = Instant::now();
        let result = self.transcribe_audio_inner(source, referer).await;
        self.timing.record("asr_transcribe", t0.elapsed());
        result
    }

    async fn transcribe_audio_inner(&self, source: &str, referer: &str) -> Result<String> {
        if !self.config.use_local_asr {
            anyhow::bail!(MediaUnavailable(
                "local video transcription is disabled for this media request".into()
            ));
        }
        let source_path = self.local_audio_source(source, referer).await?;
        tokio::time::timeout(
            Duration::from_secs(self.config.asr_timeout_s.max(60)),
            transcribe_local_file(&source_path, self.config.max_audio_seconds),
        )
        .await
        .with_context(|| {
            format!(
                "local Qwen3-ASR timed out after {}s",
                self.config.asr_timeout_s.max(60)
            )
        })?
    }

    async fn local_audio_source(&self, source: &str, referer: &str) -> Result<PathBuf> {
        let value = source.trim();
        if value.is_empty() {
            anyhow::bail!("audio source is required");
        }
        if value.starts_with("http://") || value.starts_with("https://") {
            self.download_file(value, referer, "audio", &url_suffix(value, ".mp4"))
                .await
        } else {
            Ok(PathBuf::from(value))
        }
    }
}
