//! Grok API SSE 客户端模块
//!
//! 负责与 Grok API 通信，支持 SSE 流式响应和指数退避重试。

use crate::prompt::search_prompt;
use chrono::{DateTime, Utc};
use reqwest::{header::RETRY_AFTER, StatusCode};
use serde_json::json;
use std::env;
use std::error::Error;
use std::time::Duration;
use tokio::time::sleep;

type GrokResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Grok API 配置
#[derive(Debug, Clone)]
pub struct GrokConfig {
    pub api_url: String,
    pub api_key: String,
    pub fast_model: String,
    pub deep_model: String,
}

impl GrokConfig {
    pub fn from_env() -> Self {
        let api_url = env::var("GROK_API_URL").expect("GROK_API_URL 环境变量未设置");
        let api_key = env::var("GROK_API_KEY").expect("GROK_API_KEY 环境变量未设置");
        let fast_model = env::var("GROK_FAST_MODEL")
            .or_else(|_| env::var("GROK_MODEL"))
            .unwrap_or_else(|_| "grok-4.1-fast".to_string());
        let deep_model = env::var("GROK_DEEP_MODEL").unwrap_or_else(|_| "grok-4.20-beta".to_string());

        Self {
            api_url,
            api_key,
            fast_model,
            deep_model,
        }
    }
}

/// Grok API 客户端
#[derive(Clone)]
pub struct GrokClient {
    pub config: GrokConfig,
    pub fast_http_client: reqwest::Client,
    pub deep_http_client: reqwest::Client,
}

impl GrokClient {
    const MAX_RETRIES: u32 = 3;

    pub fn new(config: GrokConfig) -> Self {
        let fast_http_client = reqwest::Client::builder()
            .read_timeout(Duration::from_secs(60))
            .build()
            .expect("创建 fast HTTP 客户端失败");
        let deep_http_client = reqwest::Client::builder()
            .read_timeout(Duration::from_secs(300))
            .build()
            .expect("创建 deep HTTP 客户端失败");

        Self {
            config,
            fast_http_client,
            deep_http_client,
        }
    }

    pub async fn fast_search(&self, query: &str, platform: Option<&str>) -> GrokResult<String> {
        #[cfg(not(test))]
        {
            self.search(
                query,
                platform,
                &self.config.fast_model,
                &self.fast_http_client,
            )
            .await
        }

        #[cfg(test)]
        {
            self.search_with_model(
                query,
                platform,
                &self.config.fast_model,
                &self.fast_http_client,
            )
            .await
        }
    }

    pub async fn deep_search(&self, query: &str, platform: Option<&str>) -> GrokResult<String> {
        #[cfg(not(test))]
        {
            self.search(
                query,
                platform,
                &self.config.deep_model,
                &self.deep_http_client,
            )
            .await
        }

        #[cfg(test)]
        {
            self.search_with_model(
                query,
                platform,
                &self.config.deep_model,
                &self.deep_http_client,
            )
            .await
        }
    }

    #[cfg(not(test))]
    async fn search(
        &self,
        query: &str,
        platform: Option<&str>,
        model: &str,
        http_client: &reqwest::Client,
    ) -> GrokResult<String> {
        self.search_with_model(query, platform, model, http_client)
            .await
    }

    #[cfg(test)]
    pub(crate) async fn search(&self, query: &str, platform: Option<&str>) -> GrokResult<String> {
        self.fast_search(query, platform).await
    }

    async fn search_with_model(
        &self,
        query: &str,
        platform: Option<&str>,
        model: &str,
        http_client: &reqwest::Client,
    ) -> GrokResult<String> {
        let endpoint = format!(
            "{}/chat/completions",
            self.config.api_url.trim_end_matches('/')
        );
        let system_prompt = search_prompt(query, platform);
        let payload = json!({
            "model": model,
            "messages": [
                {
                    "role": "system",
                    "content": system_prompt,
                },
                {
                    "role": "user",
                    "content": query,
                }
            ],
            "stream": true,
            "temperature": 0
        });

        let mut retries = 0_u32;
        let mut backoff_secs = 1_u64;

        loop {
            let response = http_client
                .post(&endpoint)
                .bearer_auth(&self.config.api_key)
                .header("Content-Type", "application/json")
                .json(&payload)
                .send()
                .await?;

            let status = response.status();
            if status.is_success() {
                return Self::parse_sse_stream(response).await;
            }

            let headers = response.headers().clone();
            let body = response.text().await.unwrap_or_default();

            if Self::should_retry(status) && retries < Self::MAX_RETRIES {
                let delay = Self::retry_delay(&headers)
                    .unwrap_or_else(|| Duration::from_secs(backoff_secs));
                retries += 1;
                tracing::warn!(
                    attempt = retries,
                    status = status.as_u16(),
                    delay_secs = delay.as_secs_f64(),
                    "Grok 请求失败，准备重试"
                );
                sleep(delay).await;
                backoff_secs = backoff_secs.saturating_mul(2);
                continue;
            }

            return Err(
                format!("Grok API 请求失败，状态码 {}，响应体: {}", status.as_u16(), body).into(),
            );
        }
    }

    fn should_retry(status: StatusCode) -> bool {
        status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
    }

    fn retry_delay(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
        let retry_after = headers.get(RETRY_AFTER)?;
        let retry_after_str = retry_after.to_str().ok()?.trim();

        if let Ok(seconds) = retry_after_str.parse::<u64>() {
            return Some(Duration::from_secs(seconds));
        }

        let date_time = DateTime::parse_from_rfc2822(retry_after_str).ok()?;
        let delay = date_time.with_timezone(&Utc) - Utc::now();
        Some(Duration::from_secs(delay.num_seconds().max(0) as u64))
    }

    async fn parse_sse_stream(mut response: reqwest::Response) -> GrokResult<String> {
        let mut content = String::new();
        let mut line_buffer = String::new();

        while let Some(chunk) = response.chunk().await? {
            line_buffer.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(newline_pos) = line_buffer.find('\n') {
                let mut raw_line: String = line_buffer.drain(..=newline_pos).collect();
                if raw_line.ends_with('\n') {
                    raw_line.pop();
                }
                if raw_line.ends_with('\r') {
                    raw_line.pop();
                }

                if Self::handle_sse_line(raw_line.trim(), &mut content) {
                    return Ok(content);
                }
            }
        }

        let remaining = line_buffer.trim();
        if !remaining.is_empty() {
            let _ = Self::handle_sse_line(remaining, &mut content);
        }

        Ok(content)
    }

    fn handle_sse_line(line: &str, content: &mut String) -> bool {
        if line.is_empty() || !line.starts_with("data:") {
            return false;
        }

        let data = line[5..].trim_start();
        if data == "[DONE]" {
            return true;
        }
        if data.is_empty() {
            return false;
        }

        match serde_json::from_str::<serde_json::Value>(data) {
            Ok(value) => {
                if let Some(delta_content) = value
                    .get("choices")
                    .and_then(|choices| choices.as_array())
                    .and_then(|choices| choices.first())
                    .and_then(|choice| choice.get("delta"))
                    .and_then(|delta| delta.get("content"))
                    .and_then(|delta_content| delta_content.as_str())
                {
                    content.push_str(delta_content);
                }
            }
            Err(error) => {
                tracing::debug!(
                    error = %error,
                    line = data,
                    "SSE 数据不是可解析 JSON，已忽略"
                );
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::Any;
    use std::panic::{self, AssertUnwindSafe};
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[allow(unused_unsafe)]
    fn set_env_var(key: &str, value: &str) {
        unsafe {
            env::set_var(key, value);
        }
    }

    #[allow(unused_unsafe)]
    fn remove_env_var(key: &str) {
        unsafe {
            env::remove_var(key);
        }
    }

    #[allow(unused_unsafe)]
    fn restore_env_var(key: &str, value: Option<String>) {
        match value {
            Some(current) => unsafe { env::set_var(key, current) },
            None => unsafe { env::remove_var(key) },
        }
    }

    fn panic_message(payload: Box<dyn Any + Send>) -> String {
        if let Some(message) = payload.downcast_ref::<String>() {
            return message.clone();
        }
        if let Some(message) = payload.downcast_ref::<&str>() {
            return (*message).to_string();
        }
        "<unknown panic>".to_string()
    }

    #[test]
    fn test_from_env_panics_on_missing_url() {
        let _guard = env_lock().lock().expect("failed to acquire env lock");
        let original_url = env::var("GROK_API_URL").ok();
        let original_key = env::var("GROK_API_KEY").ok();
        let original_model = env::var("GROK_MODEL").ok();
        let original_fast_model = env::var("GROK_FAST_MODEL").ok();
        let original_deep_model = env::var("GROK_DEEP_MODEL").ok();

        remove_env_var("GROK_API_URL");
        set_env_var("GROK_API_KEY", "test-key");
        remove_env_var("GROK_MODEL");
        remove_env_var("GROK_FAST_MODEL");
        remove_env_var("GROK_DEEP_MODEL");

        let result = panic::catch_unwind(AssertUnwindSafe(GrokConfig::from_env));

        restore_env_var("GROK_API_URL", original_url);
        restore_env_var("GROK_API_KEY", original_key);
        restore_env_var("GROK_MODEL", original_model);
        restore_env_var("GROK_FAST_MODEL", original_fast_model);
        restore_env_var("GROK_DEEP_MODEL", original_deep_model);

        assert!(result.is_err());
        let message = panic_message(result.expect_err("should panic"));
        assert!(message.contains("GROK_API_URL"));
    }

    #[test]
    fn test_from_env_defaults_model() {
        let _guard = env_lock().lock().expect("failed to acquire env lock");
        let original_url = env::var("GROK_API_URL").ok();
        let original_key = env::var("GROK_API_KEY").ok();
        let original_model = env::var("GROK_MODEL").ok();
        let original_fast_model = env::var("GROK_FAST_MODEL").ok();
        let original_deep_model = env::var("GROK_DEEP_MODEL").ok();

        set_env_var("GROK_API_URL", "https://api.x.ai/v1");
        set_env_var("GROK_API_KEY", "test-key");
        remove_env_var("GROK_MODEL");
        remove_env_var("GROK_FAST_MODEL");
        remove_env_var("GROK_DEEP_MODEL");

        let config = GrokConfig::from_env();

        restore_env_var("GROK_API_URL", original_url);
        restore_env_var("GROK_API_KEY", original_key);
        restore_env_var("GROK_MODEL", original_model);
        restore_env_var("GROK_FAST_MODEL", original_fast_model);
        restore_env_var("GROK_DEEP_MODEL", original_deep_model);

        assert_eq!(config.fast_model, "grok-4.1-fast");
        assert_eq!(config.deep_model, "grok-4.20-beta");
    }

    #[test]
    fn test_from_env_fast_model_fallback_to_grok_model() {
        let _guard = env_lock().lock().expect("failed to acquire env lock");
        let original_url = env::var("GROK_API_URL").ok();
        let original_key = env::var("GROK_API_KEY").ok();
        let original_model = env::var("GROK_MODEL").ok();
        let original_fast_model = env::var("GROK_FAST_MODEL").ok();
        let original_deep_model = env::var("GROK_DEEP_MODEL").ok();

        set_env_var("GROK_API_URL", "https://api.x.ai/v1");
        set_env_var("GROK_API_KEY", "test-key");
        set_env_var("GROK_MODEL", "grok-fallback-model");
        remove_env_var("GROK_FAST_MODEL");
        remove_env_var("GROK_DEEP_MODEL");

        let config = GrokConfig::from_env();

        restore_env_var("GROK_API_URL", original_url);
        restore_env_var("GROK_API_KEY", original_key);
        restore_env_var("GROK_MODEL", original_model);
        restore_env_var("GROK_FAST_MODEL", original_fast_model);
        restore_env_var("GROK_DEEP_MODEL", original_deep_model);

        assert_eq!(config.fast_model, "grok-fallback-model");
        assert_eq!(config.deep_model, "grok-4.20-beta");
    }

    #[test]
    fn test_from_env_custom_models() {
        let _guard = env_lock().lock().expect("failed to acquire env lock");
        let original_url = env::var("GROK_API_URL").ok();
        let original_key = env::var("GROK_API_KEY").ok();
        let original_model = env::var("GROK_MODEL").ok();
        let original_fast_model = env::var("GROK_FAST_MODEL").ok();
        let original_deep_model = env::var("GROK_DEEP_MODEL").ok();

        set_env_var("GROK_API_URL", "https://api.x.ai/v1");
        set_env_var("GROK_API_KEY", "test-key");
        set_env_var("GROK_MODEL", "legacy-grok-model");
        set_env_var("GROK_FAST_MODEL", "grok-fast-custom");
        set_env_var("GROK_DEEP_MODEL", "grok-deep-custom");

        let config = GrokConfig::from_env();

        restore_env_var("GROK_API_URL", original_url);
        restore_env_var("GROK_API_KEY", original_key);
        restore_env_var("GROK_MODEL", original_model);
        restore_env_var("GROK_FAST_MODEL", original_fast_model);
        restore_env_var("GROK_DEEP_MODEL", original_deep_model);

        assert_eq!(config.fast_model, "grok-fast-custom");
        assert_eq!(config.deep_model, "grok-deep-custom");
    }

    #[test]
    fn test_should_retry_policy() {
        assert!(GrokClient::should_retry(StatusCode::TOO_MANY_REQUESTS));
        assert!(GrokClient::should_retry(StatusCode::INTERNAL_SERVER_ERROR));
        assert!(!GrokClient::should_retry(StatusCode::BAD_REQUEST));
    }

    #[test]
    fn test_no_println_grok() {
        let source = include_str!("grok.rs");
        let println_macro = ["print", "ln!"].concat();
        let dbg_macro = ["db", "g!"].concat();
        assert!(!source.contains(&println_macro));
        assert!(!source.contains(&dbg_macro));
    }
}
