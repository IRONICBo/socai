use std::io::Write;
use std::time::Duration;

use base64::Engine;
use serde::Serialize;

const MAX_WAV_BYTES: usize = 4 * 1024 * 1024;
const EXPECTED_SAMPLE_RATE: u32 = 16_000;
const MAX_RECORDING_SECONDS: u64 = 120;

#[derive(Serialize)]
pub struct VoiceInputStatus {
    ready: bool,
    route: String,
    state: String,
    local_state: String,
    downloaded_bytes: u64,
    total_bytes: u64,
    error: Option<String>,
}

#[derive(Serialize)]
pub struct VoiceTranscript {
    text: String,
}

#[tauri::command]
pub async fn voice_input_status() -> Result<VoiceInputStatus, String> {
    let access = socai_core::cloud::paid_asr_access().await;
    if access.as_ref().is_ok_and(|access| access.ready) {
        return Ok(VoiceInputStatus {
            ready: true,
            route: "cloud".into(),
            state: "cloud_ready".into(),
            local_state: "not_checked".into(),
            downloaded_bytes: 0,
            total_bytes: 0,
            error: None,
        });
    }

    let local = socai_core::media::local_asr_status()
        .await
        .map_err(|error| format!("failed to inspect local ASR: {error:#}"))?;
    let (state, error) = match access {
        Ok(access) if !access.logged_in => ("login_required", None),
        Ok(access) if !access.active_subscription => ("subscription_required", None),
        Ok(access) if access.balance_points <= 0 => ("credits_required", None),
        Ok(_) => ("local_only", None),
        Err(error) => ("billing_unavailable", Some(format!("{error:#}"))),
    };
    Ok(VoiceInputStatus {
        ready: false,
        route: "local".into(),
        state: state.into(),
        local_state: local.state,
        downloaded_bytes: local.downloaded_bytes,
        total_bytes: local.total_bytes,
        error: error.or(local.error),
    })
}

#[tauri::command]
pub async fn voice_input_transcribe(audio_base64: String) -> Result<VoiceTranscript, String> {
    let access = socai_core::cloud::paid_asr_access()
        .await
        .map_err(|error| format!("could not verify cloud ASR access: {error:#}"))?;
    if !access.ready {
        return Err("paid cloud ASR is not available for this account".into());
    }
    if audio_base64.len() > MAX_WAV_BYTES.saturating_mul(4).div_ceil(3) + 16 {
        return Err("recorded audio is too large".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(audio_base64.as_bytes())
        .map_err(|_| "recorded audio is not valid base64".to_string())?;
    let duration_s = validate_pcm_wav(&bytes)?;

    let mut audio = tempfile::Builder::new()
        .prefix("socai-voice-")
        .suffix(".wav")
        .tempfile()
        .map_err(|error| format!("could not create temporary voice recording: {error}"))?;
    audio
        .write_all(&bytes)
        .map_err(|error| format!("could not save temporary voice recording: {error}"))?;
    audio
        .flush()
        .map_err(|error| format!("could not flush temporary voice recording: {error}"))?;

    let client_task_id = format!("voice-{}", uuid::Uuid::new_v4());
    let result = socai_core::cloud::transcribe_audio_file(
        audio.path(),
        duration_s as i64,
        Duration::from_secs(180),
        Some(&client_task_id),
    )
    .await;
    let final_status = if result.is_ok() {
        "completed"
    } else {
        "failed"
    };
    // Settlement is idempotent. If these retries all hit a transient network
    // error, the durable server-side task remains available for stale-task
    // recovery; never discard a transcript the provider already produced.
    let _settlement =
        crate::commands::settle_hosted_task_with_retry(&client_task_id, final_status).await;

    match result {
        Ok(result) => Ok(VoiceTranscript {
            text: result.transcript.trim().to_string(),
        }),
        Err(error) => Err(format!("{error:#}")),
    }
}

fn validate_pcm_wav(bytes: &[u8]) -> Result<u64, String> {
    if bytes.len() < 44 || bytes.len() > MAX_WAV_BYTES {
        return Err("recorded audio has an invalid size".into());
    }
    if bytes.get(0..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return Err("recorded audio is not a WAV file".into());
    }
    let riff_bytes = u32::from_le_bytes(
        bytes[4..8]
            .try_into()
            .map_err(|_| "recorded WAV header is invalid")?,
    ) as usize;
    if riff_bytes.checked_add(8) != Some(bytes.len()) {
        return Err("recorded WAV length does not match its header".into());
    }

    let mut cursor = 12usize;
    let mut format = None;
    let mut data_bytes = None;
    while cursor.saturating_add(8) <= bytes.len() {
        let id = &bytes[cursor..cursor + 4];
        let size = u32::from_le_bytes(
            bytes[cursor + 4..cursor + 8]
                .try_into()
                .map_err(|_| "recorded WAV chunk is invalid")?,
        ) as usize;
        let start = cursor + 8;
        let end = start
            .checked_add(size)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| "recorded WAV chunk is truncated".to_string())?;
        if id == b"fmt " {
            if format.is_some() {
                return Err("recorded WAV has duplicate format chunks".into());
            }
            if size < 16 {
                return Err("recorded WAV format chunk is too short".into());
            }
            let audio_format = u16::from_le_bytes([bytes[start], bytes[start + 1]]);
            let channels = u16::from_le_bytes([bytes[start + 2], bytes[start + 3]]);
            let sample_rate = u32::from_le_bytes(
                bytes[start + 4..start + 8]
                    .try_into()
                    .map_err(|_| "recorded WAV format is invalid")?,
            );
            let byte_rate = u32::from_le_bytes(
                bytes[start + 8..start + 12]
                    .try_into()
                    .map_err(|_| "recorded WAV byte rate is invalid")?,
            );
            let block_align = u16::from_le_bytes([bytes[start + 12], bytes[start + 13]]);
            let bits_per_sample = u16::from_le_bytes([bytes[start + 14], bytes[start + 15]]);
            format = Some((
                audio_format,
                channels,
                sample_rate,
                byte_rate,
                block_align,
                bits_per_sample,
            ));
        } else if id == b"data" {
            if data_bytes.is_some() {
                return Err("recorded WAV has duplicate audio data chunks".into());
            }
            data_bytes = Some(size);
        }
        cursor = end.saturating_add(size % 2);
    }
    if cursor != bytes.len() {
        return Err("recorded WAV has trailing or incomplete chunk data".into());
    }

    let (audio_format, channels, sample_rate, byte_rate, block_align, bits_per_sample) =
        format.ok_or_else(|| "recorded WAV has no format chunk".to_string())?;
    if audio_format != 1
        || channels != 1
        || sample_rate != EXPECTED_SAMPLE_RATE
        || byte_rate != EXPECTED_SAMPLE_RATE * 2
        || block_align != 2
        || bits_per_sample != 16
    {
        return Err("recorded audio must be mono 16 kHz PCM WAV".into());
    }
    let data_bytes = data_bytes.ok_or_else(|| "recorded WAV has no audio data".to_string())?;
    if data_bytes % usize::from(block_align) != 0 {
        return Err("recorded WAV audio data is not sample-aligned".into());
    }
    let bytes_per_second = u64::from(sample_rate) * u64::from(channels) * 2;
    let duration_s = (data_bytes as u64).div_ceil(bytes_per_second);
    if duration_s == 0 {
        return Err("recorded audio is empty".into());
    }
    if duration_s > MAX_RECORDING_SECONDS {
        return Err("recorded audio is longer than two minutes".into());
    }
    Ok(duration_s)
}

#[cfg(test)]
mod tests {
    use super::validate_pcm_wav;

    fn wav(samples: usize) -> Vec<u8> {
        let data_bytes = samples * 2;
        let mut bytes = vec![0; 44 + data_bytes];
        bytes[0..4].copy_from_slice(b"RIFF");
        bytes[4..8].copy_from_slice(&(36 + data_bytes as u32).to_le_bytes());
        bytes[8..12].copy_from_slice(b"WAVE");
        bytes[12..16].copy_from_slice(b"fmt ");
        bytes[16..20].copy_from_slice(&16u32.to_le_bytes());
        bytes[20..22].copy_from_slice(&1u16.to_le_bytes());
        bytes[22..24].copy_from_slice(&1u16.to_le_bytes());
        bytes[24..28].copy_from_slice(&16_000u32.to_le_bytes());
        bytes[28..32].copy_from_slice(&32_000u32.to_le_bytes());
        bytes[32..34].copy_from_slice(&2u16.to_le_bytes());
        bytes[34..36].copy_from_slice(&16u16.to_le_bytes());
        bytes[36..40].copy_from_slice(b"data");
        bytes[40..44].copy_from_slice(&(data_bytes as u32).to_le_bytes());
        bytes
    }

    #[test]
    fn validates_the_browser_recording_contract() {
        assert_eq!(validate_pcm_wav(&wav(16_000)).unwrap(), 1);
        assert_eq!(validate_pcm_wav(&wav(16_001)).unwrap(), 2);
    }

    #[test]
    fn rejects_non_wav_audio() {
        assert!(validate_pcm_wav(&vec![0; 44]).is_err());
    }

    #[test]
    fn rejects_duplicate_data_chunks_and_inconsistent_lengths() {
        let mut duplicate = wav(16_000);
        duplicate.extend_from_slice(b"data");
        duplicate.extend_from_slice(&0u32.to_le_bytes());
        let riff_size = (duplicate.len() - 8) as u32;
        duplicate[4..8].copy_from_slice(&riff_size.to_le_bytes());
        assert!(validate_pcm_wav(&duplicate).is_err());

        let mut inconsistent = wav(16_000);
        inconsistent[4..8].copy_from_slice(&36u32.to_le_bytes());
        assert!(validate_pcm_wav(&inconsistent).is_err());

        let mut short_format = wav(16_000);
        short_format.splice(12..12, [b'f', b'm', b't', b' ', 0, 0, 0, 0]);
        let riff_size = (short_format.len() - 8) as u32;
        short_format[4..8].copy_from_slice(&riff_size.to_le_bytes());
        assert!(validate_pcm_wav(&short_format).is_err());
    }
}
