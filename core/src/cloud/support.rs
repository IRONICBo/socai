use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;

use std::sync::LazyLock;
use std::time::Duration;

use super::auth::{configured_base_url, diagnostic_device_token, require_success};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorDiagnosis {
    pub title: String,
    pub message: String,
    pub action: String,
}

pub async fn diagnose_error(error: &str, language: &str) -> Result<ErrorDiagnosis> {
    let base_url = configured_base_url()
        .ok_or_else(|| anyhow::anyhow!("socai service URL is not configured"))?;
    let device_token = diagnostic_device_token()
        .ok_or_else(|| anyhow::anyhow!("device registration is required"))?;
    let response = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(35))
        .build()?
        .post(format!("{base_url}/v1/support/diagnose-error"))
        .bearer_auth(device_token)
        .json(&json!({
            "error": sanitize_error(error),
            "language": if language == "en" { "en" } else { "zh" },
        }))
        .send()
        .await
        .context("failed to diagnose error")?;
    Ok(require_success(response, "error diagnosis")
        .await?
        .json()
        .await?)
}

fn sanitize_error(error: &str) -> String {
    static JSON_SECRET: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?i)([\"'](?:api[_ -]?key|access[_ -]?token|refresh[_ -]?token|token|secret|client[_ -]?(?:id|secret)|credential|authorization|cookie|x-api-key|session|password|passwd)[\"']\s*:\s*)(?:\"[^\"]*\"|'[^']*'|[^\s,;}\]]+)"#,
        )
        .expect("valid error sanitizer")
    });
    static LINE_SECRET: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?im)(\b(?:authorization|proxy-authorization|cookie|set-cookie|x-api-key|session|password|passwd|credential|client[_ -]?id)\s*[:=]\s*).+$",
        )
        .expect("valid error sanitizer")
    });
    static INLINE_SECRET: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?i)(\b(?:api[_ -]?key|access[_ -]?token|refresh[_ -]?token|token|secret|client[_ -]?(?:id|secret)|credential|authorization|cookie|x-api-key|session)\s*[:=]\s*)(?:"[^"]*"|'[^']*'|[^\s,;]+)"#,
        )
        .expect("valid error sanitizer")
    });
    static WINDOWS_PATH: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"[A-Za-z]:[\\/][^\s,;]+").expect("valid error sanitizer"));
    static UNIX_PATH: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?m)(^|[\s(=:'"])(/(?:[^/\s,;]+/)+[^/\s,;]+)"#)
            .expect("valid error sanitizer")
    });
    static URL: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\b[a-z][a-z0-9+.-]*://[^\s,;]+").expect("valid error sanitizer")
    });
    static AWS_ACCESS_KEY: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\b(?:AKIA|ASIA)[A-Z0-9]{16}\b").expect("valid error sanitizer")
    });
    static EMAIL: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b").expect("valid error sanitizer")
    });
    static PHONE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?:\+?86[- ]?)?1[3-9]\d{9}").expect("valid error sanitizer"));

    let text = crate::telemetry::redact_secrets(error);
    let text = JSON_SECRET.replace_all(&text, "${1}\"[redacted]\"");
    let text = LINE_SECRET.replace_all(&text, "${1}[redacted]");
    let text = INLINE_SECRET.replace_all(&text, "${1}[redacted]");
    let text = AWS_ACCESS_KEY.replace_all(&text, "[redacted]");
    let text = URL.replace_all(&text, "[url]");
    let text = WINDOWS_PATH.replace_all(&text, "[local-path]");
    let text = UNIX_PATH.replace_all(&text, "${1}[local-path]");
    let text = EMAIL.replace_all(&text, "[email]");
    let text = PHONE.replace_all(&text, "[phone]");
    text.chars().take(2000).collect()
}
