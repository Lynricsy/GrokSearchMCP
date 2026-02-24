mod grok;
mod parser;
mod prompt;

use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars,
    tool, tool_handler, tool_router,
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WebSearchArgs {
    pub query: String,
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub include_sources: bool,
}

#[derive(Clone)]
pub struct GrokSearchServer {
    grok_client: Arc<grok::GrokClient>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl GrokSearchServer {
    pub fn new(grok_client: grok::GrokClient) -> Self {
        Self {
            grok_client: Arc::new(grok_client),
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        name = "web_search",
        description = "Performs a deep web search based on the given query and returns Grok's answer directly."
    )]
    async fn web_search(
        &self,
        Parameters(params): Parameters<WebSearchArgs>,
    ) -> Result<CallToolResult, McpError> {
        let raw_content = match self
            .grok_client
            .search(&params.query, params.platform.as_deref())
            .await
        {
            Ok(content) => content,
            Err(error) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Grok API 请求失败: {error}"
                ))]));
            }
        };

        let clean_answer = parser::strip_sources(&raw_content).trim().to_string();

        if !params.include_sources {
            return Ok(CallToolResult::success(vec![Content::text(clean_answer)]));
        }

        let sources = parser::parse_sources(&raw_content);
        if sources.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(clean_answer)]));
        }

        let formatted_sources = sources
            .iter()
            .enumerate()
            .map(|(index, source)| {
                let title = if source.title.trim().is_empty() {
                    source.url.trim()
                } else {
                    source.title.trim()
                };
                format!("{}. [{}]({})", index + 1, title, source.url.trim())
            })
            .collect::<Vec<_>>()
            .join("\n");

        let output = if clean_answer.is_empty() {
            format!("## Sources ({})\n\n{}", sources.len(), formatted_sources)
        } else {
            format!(
                "{}\n\n---\n\n## Sources ({})\n\n{}",
                clean_answer,
                sources.len(),
                formatted_sources
            )
        };

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }
}

#[tool_handler]
impl ServerHandler for GrokSearchServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            server_info: Implementation {
                name: "grok-search-mcp".to_string(),
                title: None,
                version: env!("CARGO_PKG_VERSION").to_string(),
                description: None,
                icons: None,
                website_url: None,
            },
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability::default()),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

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

    let config = grok::GrokConfig::from_env();
    let grok_client = grok::GrokClient::new(config);
    let server = GrokSearchServer::new(grok_client);

    let transport = rmcp::transport::io::stdio();
    let server = server.serve(transport).await.expect("MCP 服务启动失败");
    server.waiting().await.expect("MCP 服务意外终止");

    tracing::info!("GrokSearchMCP 服务器已停止");
}
