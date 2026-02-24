mod grok;
mod parser;
mod prompt;

use rmcp::{
    ErrorData as McpError, Peer, RoleServer, ServerHandler, ServiceExt,
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars,
    tool, tool_handler, tool_router,
};
use serde::Deserialize;
use std::error::Error;
use std::sync::Arc;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchArgs {
    pub query: String,
    /// 聚焦搜索平台（如 "twitter", "reddit"）
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub include_sources: bool,
}

#[derive(Clone)]
pub struct GrokSearchServer {
    grok_client: Arc<grok::GrokClient>,
    tool_router: ToolRouter<Self>,
    peer: Arc<tokio::sync::RwLock<Option<Peer<RoleServer>>>>,
}

#[tool_router]
impl GrokSearchServer {
    pub fn new(grok_client: grok::GrokClient) -> Self {
        Self {
            grok_client: Arc::new(grok_client),
            tool_router: Self::tool_router(),
            peer: Arc::new(tokio::sync::RwLock::new(None)),
        }
    }

    #[tool(
        name = "fast_search",
        description = "Performs a fast web search based on the given query and returns Grok's answer directly. Supports targeted searching on specific platforms (e.g., Twitter, Reddit). Best for: simple factual queries, quick lookups, recent news, definitions, straightforward questions. Response time: typically 5-15 seconds. Note: For in-depth research or multi-faceted analysis, use deep_search instead."
    )]
    async fn fast_search(
        &self,
        Parameters(params): Parameters<SearchArgs>,
    ) -> Result<CallToolResult, McpError> {
        let raw_content = self
            .grok_client
            .fast_search(&params.query, params.platform.as_deref())
            .await;

        Self::handle_search_result(raw_content, params.include_sources)
    }

    #[tool(
        name = "deep_search",
        description = "Performs a deep web search based on the given query using Grok's multi-agent cluster for thorough analysis. Supports targeted searching on specific platforms (e.g., Twitter, Reddit). Best for: complex research questions, multi-faceted topics, comparative analysis, in-depth investigations requiring comprehensive coverage. Note: takes longer (30-120 seconds) but provides more thorough results. Use fast_search for simple queries."
    )]
    async fn deep_search(
        &self,
        Parameters(params): Parameters<SearchArgs>,
        meta: Meta,
        peer: Peer<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        self.set_peer(peer).await;
        let progress_token = meta.get_progress_token();
        let stored_peer = self.peer.read().await.clone();
        let raw_content = self
            .grok_client
            .deep_search_with_progress(
                &params.query,
                params.platform.as_deref(),
                stored_peer,
                progress_token,
            )
            .await;

        Self::handle_search_result(raw_content, params.include_sources)
    }

    fn handle_search_result(
        raw_content: Result<String, Box<dyn Error + Send + Sync>>,
        include_sources: bool,
    ) -> Result<CallToolResult, McpError> {
        let raw_content = match raw_content {
            Ok(content) => content,
            Err(error) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Grok API 请求失败: {error}"
                ))]));
            }
        };

        let clean_answer = parser::strip_sources(&raw_content).trim().to_string();

        if !include_sources {
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

    async fn set_peer(&self, peer: Peer<RoleServer>) {
        let mut guard = self.peer.write().await;
        *guard = Some(peer);
    }
}

#[tool_handler]
impl ServerHandler for GrokSearchServer {
    async fn initialize(
        &self,
        request: InitializeRequestParams,
        context: rmcp::service::RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        self.set_peer(context.peer.clone()).await;
        if context.peer.peer_info().is_none() {
            context.peer.set_peer_info(request);
        }
        Ok(self.get_info())
    }

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
