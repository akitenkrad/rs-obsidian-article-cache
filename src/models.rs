use serde::Serialize;

/// Markdown fileから抽出された論文メタデータ
#[derive(Debug, Clone)]
pub struct Paper {
    pub file_path: String,
    pub title: Option<String>,
    pub year: Option<i32>,
    pub venue: Option<String>,
    pub doi: Option<String>,
    pub arxiv_id: Option<String>,
    pub citation_count: Option<i32>,
    pub open_access: Option<bool>,
    pub status: Option<String>,
    pub authors: Vec<String>,
    pub tags: Vec<String>,
    pub fields_of_study: Vec<String>,
    pub research_tasks: Vec<String>,
    pub file_modified_at: String,
}

/// filter コマンドの結果用構造体
#[derive(Debug, Clone, Serialize)]
pub struct PaperResult {
    pub title: Option<String>,
    pub year: Option<i32>,
    pub venue: Option<String>,
    pub doi: Option<String>,
    pub status: Option<String>,
    pub file_path: String,
    pub authors: Vec<String>,
    pub tags: Vec<String>,
    pub fields_of_study: Vec<String>,
    pub research_tasks: Vec<String>,
}
