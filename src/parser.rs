//! 来源解析引擎模块
//!
//! 从 Grok 搜索响应中提取引用来源信息，支持 4 种解析策略。

/// 搜索结果来源
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Source {
    pub title: String,
    pub url: String,
}
