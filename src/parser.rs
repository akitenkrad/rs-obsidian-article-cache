use std::path::Path;

use anyhow::Result;
use regex::Regex;
use serde::Deserialize;

use crate::models::Paper;

// ---------------------------------------------------------------------------
// Frontmatter
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Default)]
pub struct Frontmatter {
    #[serde(default)]
    pub tags: Vec<String>,

    #[serde(rename = "ac-paper-status")]
    pub ac_paper_status: Option<String>,

    pub year: Option<i32>,

    #[serde(rename = "field-of-study", default)]
    pub field_of_study: FieldOfStudy,

    #[allow(dead_code)]
    pub created: Option<String>,
}

/// `field-of-study` は文字列単体・リスト・空のいずれかで出現する．
#[derive(Debug, Deserialize, Default)]
#[serde(untagged)]
pub enum FieldOfStudy {
    #[default]
    None,
    Single(String),
    Multiple(Vec<String>),
}

impl FieldOfStudy {
    pub fn into_vec(self) -> Vec<String> {
        match self {
            FieldOfStudy::None => vec![],
            FieldOfStudy::Single(s) => {
                if s.is_empty() {
                    vec![]
                } else {
                    vec![s]
                }
            }
            FieldOfStudy::Multiple(v) => v,
        }
    }
}

/// ファイル先頭の `---` ... `---` ブロックを抽出してデシリアライズする．
pub fn parse_frontmatter(content: &str) -> Result<Option<Frontmatter>> {
    // Frontmatter は必ずファイルの先頭に来る
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return Ok(None);
    }

    // 最初の `---` の直後から次の `---` までを切り出す
    let after_first = &trimmed[3..];
    let end = after_first.find("\n---");
    let yaml_block = match end {
        Some(pos) => &after_first[..pos],
        None => return Ok(None),
    };

    let fm: Frontmatter = serde_yaml::from_str(yaml_block)?;
    Ok(Some(fm))
}

// ---------------------------------------------------------------------------
// Bibliography table
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct BibliographyInfo {
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub year: Option<i32>,
    pub venue: Option<String>,
    pub doi: Option<String>,
    pub arxiv_id: Option<String>,
    pub research_tasks: Vec<String>,
    pub citation_count: Option<i32>,
    pub open_access: Option<bool>,
}

/// `## 書誌情報` セクション内の Markdown テーブルを解析する．
pub fn parse_bibliography_table(content: &str) -> BibliographyInfo {
    let mut info = BibliographyInfo::default();

    // `## 書誌情報` セクションを探す
    let section_start = match content.find("## 書誌情報") {
        Some(pos) => pos,
        None => return info,
    };

    let section = &content[section_start..];

    // 次の `## ` が来たらセクション終了
    let section_end = section[1..]
        .find("\n## ")
        .map(|p| p + 1)
        .unwrap_or(section.len());
    let section = &section[..section_end];

    // テーブル行を解析: `| **キー** | 値 |`
    let row_re = Regex::new(r"^\|\s*\*\*(.+?)\*\*\s*\|\s*(.+?)\s*\|$").unwrap();
    let citation_re = Regex::new(r"(\d[\d,]*)").unwrap();

    for line in section.lines() {
        let line = line.trim();
        if let Some(caps) = row_re.captures(line) {
            let key = caps.get(1).unwrap().as_str().trim();
            let value = caps.get(2).unwrap().as_str().trim();

            match key {
                "タイトル" => {
                    info.title = non_na(value);
                }
                "著者" => {
                    if !is_na(value) {
                        info.authors = value
                            .split(", ")
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                    }
                }
                "発行年" => {
                    if !is_na(value) {
                        info.year = value.parse::<i32>().ok();
                    }
                }
                "掲載誌/学会" => {
                    info.venue = non_na(value);
                }
                "DOI" => {
                    info.doi = non_na(value);
                }
                "arXiv ID" => {
                    info.arxiv_id = non_na(value);
                }
                "研究タスク" => {
                    if !is_na(value) {
                        info.research_tasks = value
                            .split(", ")
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                    }
                }
                "被引用数" => {
                    if !is_na(value) {
                        if let Some(m) = citation_re.find(value) {
                            let num_str = m.as_str().replace(',', "");
                            info.citation_count = num_str.parse::<i32>().ok();
                        }
                    }
                }
                "オープンアクセス" => {
                    if !is_na(value) {
                        info.open_access = Some(value == "Yes");
                    }
                }
                _ => {}
            }
        }
    }

    info
}

fn is_na(s: &str) -> bool {
    s == "N/A" || s == "n/a" || s == "-"
}

fn non_na(s: &str) -> Option<String> {
    if is_na(s) || s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

// ---------------------------------------------------------------------------
// Paper 統合パーサ
// ---------------------------------------------------------------------------

/// Frontmatter + 書誌情報テーブルから Paper を組み立てる．
/// `学術論文` タグがなければ `Ok(None)` を返す．
pub fn parse_paper(file_path: &Path, content: &str, modified_at: &str) -> Result<Option<Paper>> {
    // 1. Frontmatter 解析
    let fm = match parse_frontmatter(content)? {
        Some(fm) => fm,
        None => return Ok(None),
    };

    // タグ判定: `学術論文` がなければスキップ
    if !fm.tags.iter().any(|t| t == "学術論文") {
        return Ok(None);
    }

    // 2. 書誌情報テーブル解析
    let bib = parse_bibliography_table(content);

    // 3. year: 書誌テーブル優先，なければ frontmatter
    let year = bib.year.or(fm.year);

    // 4. title: 書誌テーブル優先，なければファイル名
    let title = bib.title.or_else(|| {
        file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
    });

    let paper = Paper {
        file_path: file_path.to_string_lossy().to_string(),
        title,
        year,
        venue: bib.venue,
        doi: bib.doi,
        arxiv_id: bib.arxiv_id,
        citation_count: bib.citation_count,
        open_access: bib.open_access,
        status: fm.ac_paper_status,
        authors: bib.authors,
        tags: fm.tags,
        fields_of_study: fm.field_of_study.into_vec(),
        research_tasks: bib.research_tasks,
        file_modified_at: modified_at.to_string(),
    };

    Ok(Some(paper))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frontmatter_basic() {
        let content = r#"---
tags:
  - 学術論文
  - IntrusionDetection
ac-paper-status: to read
year: 2002
field-of-study:
  - Computer Security
created: 2026-02-22 12:00:00
---
# Title
"#;
        let fm = parse_frontmatter(content).unwrap().unwrap();
        assert_eq!(fm.tags, vec!["学術論文", "IntrusionDetection"]);
        assert_eq!(fm.ac_paper_status.as_deref(), Some("to read"));
        assert_eq!(fm.year, Some(2002));
        let fields = fm.field_of_study.into_vec();
        assert_eq!(fields, vec!["Computer Security"]);
    }

    #[test]
    fn test_parse_frontmatter_no_frontmatter() {
        let content = "# Just a heading\nSome text.";
        let result = parse_frontmatter(content).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_frontmatter_field_of_study_single() {
        let content = "---\nfield-of-study: NLP\n---\n";
        let fm = parse_frontmatter(content).unwrap().unwrap();
        let fields = fm.field_of_study.into_vec();
        assert_eq!(fields, vec!["NLP"]);
    }

    #[test]
    fn test_parse_frontmatter_field_of_study_empty() {
        let content = "---\ntags:\n  - 学術論文\n---\n";
        let fm = parse_frontmatter(content).unwrap().unwrap();
        let fields = fm.field_of_study.into_vec();
        assert!(fields.is_empty());
    }

    #[test]
    fn test_parse_bibliography_table() {
        let content = r#"## 書誌情報

| 項目 | 内容 |
|------|------|
| **タイトル** | ADMIT: Anomaly-based Data Mining for Intrusions |
| **著者** | Karlton Sequeira, Mohammed J. Zaki |
| **発行年** | 2002 |
| **掲載誌/学会** | KDD '02 |
| **DOI** | 10.1145/775047.775103 |
| **arXiv ID** | N/A |
| **研究タスク** | Intrusion Detection, Anomaly Detection |
| **被引用数** | 289 |
| **オープンアクセス** | Yes |
"#;
        let bib = parse_bibliography_table(content);
        assert_eq!(bib.title.as_deref(), Some("ADMIT: Anomaly-based Data Mining for Intrusions"));
        assert_eq!(bib.authors, vec!["Karlton Sequeira", "Mohammed J. Zaki"]);
        assert_eq!(bib.year, Some(2002));
        assert_eq!(bib.venue.as_deref(), Some("KDD '02"));
        assert_eq!(bib.doi.as_deref(), Some("10.1145/775047.775103"));
        assert!(bib.arxiv_id.is_none());
        assert_eq!(bib.research_tasks, vec!["Intrusion Detection", "Anomaly Detection"]);
        assert_eq!(bib.citation_count, Some(289));
        assert_eq!(bib.open_access, Some(true));
    }

    #[test]
    fn test_parse_bibliography_citation_with_annotation() {
        let content = r#"## 書誌情報

| 項目 | 内容 |
|------|------|
| **被引用数** | 1,955 (Semantic Scholar, 2025-01-15) |
"#;
        let bib = parse_bibliography_table(content);
        assert_eq!(bib.citation_count, Some(1955));
    }

    #[test]
    fn test_parse_bibliography_na_fields() {
        let content = r#"## 書誌情報

| 項目 | 内容 |
|------|------|
| **DOI** | N/A |
| **arXiv ID** | N/A |
| **被引用数** | N/A |
"#;
        let bib = parse_bibliography_table(content);
        assert!(bib.doi.is_none());
        assert!(bib.arxiv_id.is_none());
        assert!(bib.citation_count.is_none());
    }

    #[test]
    fn test_parse_paper_no_academic_tag() {
        let content = "---\ntags:\n  - メモ\n---\n# Note\n";
        let result = parse_paper(Path::new("/test/note.md"), content, "2026-01-01").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_paper_complete() {
        let content = r#"---
tags:
  - 学術論文
ac-paper-status: done
year: 2001
field-of-study:
  - Computer Security
---

## 書誌情報

| 項目 | 内容 |
|------|------|
| **タイトル** | Test Paper |
| **著者** | Alice, Bob |
| **発行年** | 2002 |
| **掲載誌/学会** | ICML |
| **DOI** | 10.1234/test |
| **arXiv ID** | 2301.00001 |
| **研究タスク** | Classification |
| **被引用数** | 42 |
| **オープンアクセス** | Yes |
"#;
        let paper = parse_paper(Path::new("/test/paper.md"), content, "2026-01-01")
            .unwrap()
            .unwrap();
        assert_eq!(paper.title.as_deref(), Some("Test Paper"));
        // 書誌テーブルの year (2002) が frontmatter (2001) より優先
        assert_eq!(paper.year, Some(2002));
        assert_eq!(paper.venue.as_deref(), Some("ICML"));
        assert_eq!(paper.status.as_deref(), Some("done"));
        assert_eq!(paper.authors, vec!["Alice", "Bob"]);
        assert_eq!(paper.fields_of_study, vec!["Computer Security"]);
    }

    #[test]
    fn test_parse_paper_fallback_title_to_filename() {
        let content = "---\ntags:\n  - 学術論文\n---\n# Some heading\n";
        let paper = parse_paper(Path::new("/test/My Paper.md"), content, "2026-01-01")
            .unwrap()
            .unwrap();
        assert_eq!(paper.title.as_deref(), Some("My Paper"));
    }
}
