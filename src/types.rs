use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Post {
    /// Canonical x.com URL.
    pub url: String,
    /// Status id used for de-duplication and citation checks.
    pub id: String,
    /// Author handle without the leading @.
    pub author: String,
    /// YYYY-MM-DD in UTC, or null when unknown.
    pub date: Option<String>,
    /// Verbatim excerpt, whitespace-collapsed.
    pub excerpt: String,
    /// True when the post URL appeared in the X Search citations.
    pub verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchCall {
    pub name: String,
    pub query: Option<String>,
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Low,
    Medium,
    High,
    Xhigh,
}

impl Effort {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Xhigh => "xhigh",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortMode {
    Latest,
    Top,
}

impl SortMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Latest => "latest",
            Self::Top => "top",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CostSource {
    Api,
    Estimated,
}

impl CostSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::Estimated => "estimated",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub total_tokens: i64,
    pub x_search_calls: i64,
    pub x_posts_fetched: i64,
    pub x_users_fetched: i64,
    pub cost_usd: Option<f64>,
    pub cost_source: Option<CostSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Filters {
    pub from: Option<String>,
    pub to: Option<String>,
    pub handles: Vec<String>,
    pub exclude_handles: Vec<String>,
    pub limit: usize,
    pub sort: Option<SortMode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    pub model: String,
    pub response_id: Option<String>,
    pub created_at: String,
    pub elapsed_ms: u64,
    pub effort: Effort,
    pub max_turns: usize,
    pub filters: Filters,
    pub usage: Usage,
    pub searches: Vec<SearchCall>,
    pub citations: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResult {
    pub query: String,
    pub summary: String,
    pub posts: Vec<Post>,
    pub meta: Meta,
}
