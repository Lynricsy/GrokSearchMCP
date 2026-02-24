mod grok;
mod parser;
mod prompt;

use tracing_subscriber;

#[tokio::main]
async fn main() {
    // 所有日志输出到 stderr，避免污染 stdio MCP 协议
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
        )
        .init();

    tracing::info!("GrokSearchMCP 服务器启动中...");

    // TODO: 初始化 GrokSearchServer 并启动 MCP 服务
    tracing::info!("GrokSearchMCP 服务器已就绪");
}
