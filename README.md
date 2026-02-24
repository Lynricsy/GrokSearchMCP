# GrokSearchMCP

基于 Grok API 的轻量级 MCP 搜索服务器，使用 Rust 构建，提供 `fast_search` 和 `deep_search` 双工具。

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.75+-orange.svg)](https://www.rust-lang.org/)
[![npm](https://img.shields.io/npm/v/grok-search)](https://www.npmjs.com/package/grok-search)

---

## 功能特性

- **双工具设计**：提供 `fast_search`（快速搜索）和 `deep_search`（深度搜索）两种工具，分别适用于不同搜索场景
- **SSE 流式传输**：通过 Server-Sent Events 实时接收 Grok 响应
- **来源解析**：4 策略引擎自动从响应中提取引用来源（支持函数调用、标题块、Details 块、尾部链接块）
- **智能重试**：指数退避重试，支持 `Retry-After` 响应头解析
- **时间上下文注入**：自动将当前时间注入到搜索提示词
- **MCP 进度通知**：深度搜索期间通过 MCP Progress Notifications 保持连接活跃，防止超时
- **零 println**：所有日志输出到 stderr，stdout 仅用于 MCP 协议通信

---

## 环境变量

| 变量名 | 是否必需 | 说明 |
|--------|----------|------|
| `GROK_API_URL` | **必需** | Grok API 基础地址，例如 `https://api.x.ai/v1` |
| `GROK_API_KEY` | **必需** | API 密钥 |
| `GROK_FAST_MODEL` | 可选 | 快速搜索模型，回退到 `GROK_MODEL`，默认 `grok-4.1-fast` |
| `GROK_DEEP_MODEL` | 可选 | 深度搜索模型，默认 `grok-4.20-beta` |
| `GROK_MODEL` | 可选 | 向后兼容：作为 `GROK_FAST_MODEL` 的回退值 |

---

## 快速开始

无需克隆仓库，通过 `npx` 一键启动 MCP 服务器：

```bash
GROK_API_URL=https://api.x.ai/v1 \
GROK_API_KEY=your-api-key \
npx -y grok-search
```

服务器将通过 stdio 与 MCP 客户端通信。推荐配合 Claude Desktop 或 Claude Code 使用，参见下方 [MCP 配置示例](#mcp-配置示例)。

> **提示**：首次运行 `npx` 会自动下载对应平台的预编译二进制，无需安装 Rust 工具链。

---

## 安装

### 通过 NPM（推荐）

```bash
npm install -g grok-search
```

或使用 `npx` 直接运行：

```bash
npx grok-search
```

### 从源码构建

需要 Rust 1.75+，以及系统级 OpenSSL 库（Linux 上通常为 `libssl-dev`）。

```bash
cargo build --release
# 产物路径: ./target/release/grok-search-mcp
```

---

## MCP 配置示例

### Claude Desktop

编辑 `~/Library/Application Support/Claude/claude_desktop_config.json`（macOS）：

```json
{
  "mcpServers": {
    "grok-search": {
      "command": "npx",
      "args": ["-y", "grok-search"],
      "env": {
        "GROK_API_URL": "https://api.x.ai/v1",
        "GROK_API_KEY": "your-api-key"
      }
    }
  }
}
```

### Claude Code

```bash
claude mcp add grok-search -- npx -y grok-search
```

或手动配置：

```json
{
  "mcpServers": {
    "grok-search": {
      "command": "npx",
      "args": ["-y", "grok-search"],
      "env": {
        "GROK_API_URL": "https://api.x.ai/v1",
        "GROK_API_KEY": "your-api-key",
        "GROK_FAST_MODEL": "grok-4.1-fast",
        "GROK_DEEP_MODEL": "grok-4.20-beta"
      }
    }
  }
}
```

---

## 工具文档

### `fast_search`

快速网络搜索，适用于日常查询。响应速度快（通常 5-15 秒）。

**参数：**

| 参数名 | 类型 | 必需 | 默认值 | 说明 |
|--------|------|------|--------|------|
| `query` | string | **是** | — | 搜索查询语句 |
| `platform` | string | 否 | null | 聚焦搜索平台（如 `"twitter"`, `"reddit"`） |
| `include_sources` | boolean | 否 | `false` | 是否在结果中附带来源列表 |
| `include_thinking` | boolean | 否 | `false` | 是否保留模型的 `<think>` 思维链块 |

**返回：** Grok 生成的搜索答案文本；若 `include_sources` 为 `true`，则附加来源 URL 列表。

---

### `deep_search`

深度多 Agent 集群搜索，适用于复杂查询。耗时较长（通常 60-120 秒），但搜索结果更全面、更准确。搜索期间会发送 MCP 进度通知以防止客户端超时。

**参数：**

| 参数名 | 类型 | 必需 | 默认值 | 说明 |
|--------|------|------|--------|------|
| `query` | string | **是** | — | 搜索查询语句 |
| `platform` | string | 否 | null | 聚焦搜索平台（如 `"twitter"`, `"reddit"`） |
| `include_sources` | boolean | 否 | `false` | 是否在结果中附带来源列表 |
| `include_thinking` | boolean | 否 | `false` | 是否保留模型的 `<think>` 思维链块 |

**返回：** Grok 生成的搜索答案文本；若 `include_sources` 为 `true`，则附加来源 URL 列表。

---

### 工具选择指南

| 场景 | 推荐工具 | 原因 |
|------|----------|------|
| 简单事实查询 | `fast_search` | 速度快，5-15 秒内返回 |
| 最新新闻/动态 | `fast_search` | 快速获取实时信息 |
| 多源对比分析 | `deep_search` | 多 Agent 集群搜索更全面 |
| 深度研究/调研 | `deep_search` | 结果更准确、更详细 |
| 争议性话题 | `deep_search` | 多视角覆盖更完整 |

---

## 超时配置

深度搜索使用 `grok-4.20-beta` 模型（多 Agent 集群搜索），通常需要 60-120 秒。为防止超时：

### 服务器端
- 快速搜索 HTTP 超时：60 秒
- 深度搜索 HTTP 超时：300 秒
- 深度搜索期间每 5 秒发送 MCP 进度通知

### 客户端配置

| 客户端 | 超时配置 | 建议值 |
|--------|----------|--------|
| Claude Code | `MCP_TOOL_TIMEOUT` 环境变量 | `300`（秒） |
| OpenCode | 默认 300 秒 | 无需额外配置 |
| Claude Desktop | 不可配置（约 60 秒） | 依赖 MCP 进度通知保活 |

---

## License

MIT
