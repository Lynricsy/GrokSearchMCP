//! Grok API SSE 客户端模块
//!
//! 负责与 Grok API 通信，支持 SSE 流式响应和指数退避重试。

/// Grok API 配置
pub struct GrokConfig {
    pub api_url: String,
    pub api_key: String,
    pub model: String,
}

/// Grok API 客户端
pub struct GrokClient {
    pub config: GrokConfig,
    pub http_client: reqwest::Client,
}
