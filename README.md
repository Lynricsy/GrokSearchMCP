# GrokSearchMCP

基于 Grok API 的轻量级 MCP 搜索服务器，使用 Rust 构建，提供单一 `web_search` 工具。

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.75+-orange.svg)](https://www.rust-lang.org/)
[![npm](https://img.shields.io/npm/v/groks)](https://www.npmjs.com/package/groks)

---

## 功能特性

- **单工具设计**：仅暴露 `web_search`，接口简洁
- **SSE 流式传输**：通过 Server-Sent Events 实时接收 Grok 响应
- **来源解析**：4 策略引擎自动从响应中提取引用来源（支持函数调用、标题块、Details 块、尾部链接块）
- **智能重试**：指数退避重试，支持 `Retry-After` 响应头解析
- **时间上下文注入**：自动将当前时间注入到搜索提示词
- **零 println**：所有日志输出到 stderr，stdout 仅用于 MCP 协议通信

---

## 环境变量

| 变量名 | 是否必需 | 说明 |
|--------|----------|------|
| `GROK_API_URL` | **必需** | Grok API 基础地址，例如 `https://api.x.ai/v1` |
| `GROK_API_KEY` | **必需** | API 密钥 |
| `GROK_MODEL` | 可选 | 模型名称，默认 `grok-4-fast` |

---

## 快速开始

无需克隆仓库，通过 `npx` 一键启动 MCP 服务器：

```bash
GROK_API_URL=https://api.x.ai/v1 \
GROK_API_KEY=your-api-key \
npx -y groks
```

服务器将通过 stdio 与 MCP 客户端通信。推荐配合 Claude Desktop 或 Claude Code 使用，参见下方 [MCP 配置示例](#mcp-配置示例)。

> **提示**：首次运行 `npx` 会自动下载对应平台的预编译二进制，无需安装 Rust 工具链。

---

## 安装

### 通过 NPM（推荐）

```bash
npm install -g groks
```

或使用 `npx` 直接运行：

```bash
npx groks
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
      "args": ["-y", "groks"],
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
claude mcp add grok-search -- npx -y groks
```

或手动配置：

```json
{
  "mcpServers": {
    "grok-search": {
      "command": "npx",
      "args": ["-y", "groks"],
      "env": {
        "GROK_API_URL": "https://api.x.ai/v1",
        "GROK_API_KEY": "your-api-key",
        "GROK_MODEL": "grok-4-fast"
      }
    }
  }
}
```

---

## 工具文档

### `web_search`

执行深度网络搜索并返回 Grok 的答案。

**参数：**

| 参数名 | 类型 | 必需 | 默认值 | 说明 |
|--------|------|------|--------|------|
| `query` | string | **是** | — | 搜索查询语句 |
| `platform` | string | 否 | null | 聚焦搜索平台（如 `"twitter"`, `"reddit"`） |
| `include_sources` | boolean | 否 | `false` | 是否在结果中附带来源列表 |

**返回：** Grok 生成的搜索答案文本；若 `include_sources` 为 `true`，则附加来源 URL 列表。

---

## License

MIT
