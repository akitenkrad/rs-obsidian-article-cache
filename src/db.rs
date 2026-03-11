use std::collections::HashSet;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::Connection;
use strsim::normalized_levenshtein;

use crate::models::{Paper, PaperResult};

/// 統計情報
pub struct Stats {
    pub total: usize,
    pub by_status: Vec<(String, usize)>,
    pub by_year: Vec<(i32, usize)>,
    pub by_field: Vec<(String, usize)>,
    pub top_authors: Vec<(String, usize)>,
}

/// DB ファイルを開く．親ディレクトリが存在しない場合は自動作成する．
pub fn open_db(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create DB directory: {}", parent.display()))?;
    }
    let conn = Connection::open(path)
        .with_context(|| format!("Failed to open database: {}", path.display()))?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    Ok(conn)
}

/// 5 テーブル + インデックスを初期化する．
pub fn initialize_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS papers (
            id              INTEGER PRIMARY KEY,
            file_path       TEXT UNIQUE NOT NULL,
            title           TEXT,
            year            INTEGER,
            venue           TEXT,
            doi             TEXT,
            arxiv_id        TEXT,
            citation_count  INTEGER,
            open_access     BOOLEAN,
            status          TEXT,
            file_modified_at TEXT,
            cached_at       TEXT
        );

        CREATE TABLE IF NOT EXISTS authors (
            id       INTEGER PRIMARY KEY,
            paper_id INTEGER NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
            name     TEXT NOT NULL,
            position INTEGER
        );

        CREATE TABLE IF NOT EXISTS tags (
            id       INTEGER PRIMARY KEY,
            paper_id INTEGER NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
            tag      TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS fields_of_study (
            id       INTEGER PRIMARY KEY,
            paper_id INTEGER NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
            field    TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS research_tasks (
            id       INTEGER PRIMARY KEY,
            paper_id INTEGER NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
            task     TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_papers_title  ON papers(title);
        CREATE INDEX IF NOT EXISTS idx_papers_year   ON papers(year);
        CREATE INDEX IF NOT EXISTS idx_papers_status ON papers(status);
        CREATE INDEX IF NOT EXISTS idx_papers_venue  ON papers(venue);

        CREATE INDEX IF NOT EXISTS idx_authors_name         ON authors(name);
        CREATE INDEX IF NOT EXISTS idx_tags_tag             ON tags(tag);
        CREATE INDEX IF NOT EXISTS idx_fields_field         ON fields_of_study(field);
        CREATE INDEX IF NOT EXISTS idx_research_tasks_task  ON research_tasks(task);
        ",
    )?;
    Ok(())
}

/// 全テーブルを DROP する（--force 用）．
pub fn drop_all_tables(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        DROP TABLE IF EXISTS authors;
        DROP TABLE IF EXISTS tags;
        DROP TABLE IF EXISTS fields_of_study;
        DROP TABLE IF EXISTS research_tasks;
        DROP TABLE IF EXISTS papers;
        ",
    )?;
    Ok(())
}

/// Paper を DB に upsert する．トランザクション内で実行される．
pub fn upsert_paper(conn: &Connection, paper: &Paper) -> Result<()> {
    let tx = conn.unchecked_transaction()?;

    // INSERT OR REPLACE で papers テーブルに書き込み
    tx.execute(
        "INSERT OR REPLACE INTO papers
            (file_path, title, year, venue, doi, arxiv_id,
             citation_count, open_access, status, file_modified_at, cached_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        rusqlite::params![
            paper.file_path,
            paper.title,
            paper.year,
            paper.venue,
            paper.doi,
            paper.arxiv_id,
            paper.citation_count,
            paper.open_access,
            paper.status,
            paper.file_modified_at,
            Utc::now().to_rfc3339(),
        ],
    )?;

    // paper_id を取得
    let paper_id: i64 = tx.query_row(
        "SELECT id FROM papers WHERE file_path = ?1",
        [&paper.file_path],
        |row| row.get(0),
    )?;

    // 関連テーブルを一度削除して再挿入
    tx.execute("DELETE FROM authors WHERE paper_id = ?1", [paper_id])?;
    tx.execute("DELETE FROM tags WHERE paper_id = ?1", [paper_id])?;
    tx.execute(
        "DELETE FROM fields_of_study WHERE paper_id = ?1",
        [paper_id],
    )?;
    tx.execute(
        "DELETE FROM research_tasks WHERE paper_id = ?1",
        [paper_id],
    )?;

    for (i, author) in paper.authors.iter().enumerate() {
        tx.execute(
            "INSERT INTO authors (paper_id, name, position) VALUES (?1, ?2, ?3)",
            rusqlite::params![paper_id, author, i as i32],
        )?;
    }

    for tag in &paper.tags {
        tx.execute(
            "INSERT INTO tags (paper_id, tag) VALUES (?1, ?2)",
            rusqlite::params![paper_id, tag],
        )?;
    }

    for field in &paper.fields_of_study {
        tx.execute(
            "INSERT INTO fields_of_study (paper_id, field) VALUES (?1, ?2)",
            rusqlite::params![paper_id, field],
        )?;
    }

    for task in &paper.research_tasks {
        tx.execute(
            "INSERT INTO research_tasks (paper_id, task) VALUES (?1, ?2)",
            rusqlite::params![paper_id, task],
        )?;
    }

    tx.commit()?;
    Ok(())
}

/// Vault から削除されたファイルを DB から除去する．
/// 削除した件数を返す．
pub fn delete_removed_papers(
    conn: &Connection,
    existing_paths: &HashSet<String>,
) -> Result<usize> {
    let mut stmt = conn.prepare("SELECT file_path FROM papers")?;
    let db_paths: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .filter_map(|r| r.ok())
        .collect();

    let mut deleted = 0usize;
    for path in &db_paths {
        if !existing_paths.contains(path) {
            conn.execute("DELETE FROM papers WHERE file_path = ?1", [path])?;
            deleted += 1;
        }
    }

    Ok(deleted)
}

/// 指定パスの file_modified_at をキャッシュから取得する．
/// 未登録の場合は None を返す．
pub fn get_cached_modified_at(conn: &Connection, file_path: &str) -> Result<Option<String>> {
    let mut stmt = conn.prepare("SELECT file_modified_at FROM papers WHERE file_path = ?1")?;
    let result = stmt
        .query_row([file_path], |row| row.get::<_, String>(0))
        .ok();
    Ok(result)
}

// ---------------------------------------------------------------------------
// Filter
// ---------------------------------------------------------------------------

/// filter_papers 内部で使用する行タプル型．
type PaperRow = (
    i64,
    String,
    Option<String>,
    Option<i32>,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// フィルタ条件
pub struct FilterOptions {
    pub title: Option<String>,
    pub author: Option<String>,
    pub year: Option<i32>,
    pub year_from: Option<i32>,
    pub year_to: Option<i32>,
    pub keyword: Option<String>,
    pub field: Option<String>,
    pub venue: Option<String>,
    pub status: Option<String>,
    pub limit: usize,
}

/// フィルタ条件に一致する論文を検索する．
pub fn filter_papers(conn: &Connection, opts: &FilterOptions) -> Result<Vec<PaperResult>> {
    let mut sql = String::from(
        "SELECT DISTINCT p.id, p.file_path, p.title, p.year, p.venue, p.doi, p.status FROM papers p",
    );
    let mut joins: Vec<String> = Vec::new();
    let mut wheres: Vec<String> = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut param_idx = 1usize;

    // --author
    if let Some(ref author) = opts.author {
        joins.push("JOIN authors a ON a.paper_id = p.id".to_string());
        wheres.push(format!("a.name LIKE ?{}", param_idx));
        params.push(Box::new(format!("%{}%", author)));
        param_idx += 1;
    }

    // --keyword: tags OR research_tasks
    if let Some(ref keyword) = opts.keyword {
        joins.push("LEFT JOIN tags t ON t.paper_id = p.id".to_string());
        joins.push("LEFT JOIN research_tasks rt ON rt.paper_id = p.id".to_string());
        wheres.push(format!(
            "(t.tag LIKE ?{} OR rt.task LIKE ?{})",
            param_idx,
            param_idx + 1
        ));
        params.push(Box::new(format!("%{}%", keyword)));
        params.push(Box::new(format!("%{}%", keyword)));
        param_idx += 2;
    }

    // --field
    if let Some(ref field) = opts.field {
        joins.push("JOIN fields_of_study f ON f.paper_id = p.id".to_string());
        wheres.push(format!("f.field LIKE ?{}", param_idx));
        params.push(Box::new(format!("%{}%", field)));
        param_idx += 1;
    }

    // --title
    if let Some(ref title) = opts.title {
        wheres.push(format!("p.title LIKE ?{}", param_idx));
        params.push(Box::new(format!("%{}%", title)));
        param_idx += 1;
    }

    // --venue
    if let Some(ref venue) = opts.venue {
        wheres.push(format!("p.venue LIKE ?{}", param_idx));
        params.push(Box::new(format!("%{}%", venue)));
        param_idx += 1;
    }

    // --year
    if let Some(year) = opts.year {
        wheres.push(format!("p.year = ?{}", param_idx));
        params.push(Box::new(year));
        param_idx += 1;
    }

    // --year-from
    if let Some(year_from) = opts.year_from {
        wheres.push(format!("p.year >= ?{}", param_idx));
        params.push(Box::new(year_from));
        param_idx += 1;
    }

    // --year-to
    if let Some(year_to) = opts.year_to {
        wheres.push(format!("p.year <= ?{}", param_idx));
        params.push(Box::new(year_to));
        param_idx += 1;
    }

    // --status
    if let Some(ref status) = opts.status {
        wheres.push(format!("p.status = ?{}", param_idx));
        params.push(Box::new(status.clone()));
        param_idx += 1;
    }

    // SQL 組み立て
    for join in &joins {
        sql.push(' ');
        sql.push_str(join);
    }
    if !wheres.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&wheres.join(" AND "));
    }
    sql.push_str(&format!(
        " ORDER BY p.year DESC, p.title ASC LIMIT ?{}",
        param_idx
    ));
    params.push(Box::new(opts.limit as i64));

    // クエリ実行
    let param_refs: Vec<&dyn rusqlite::types::ToSql> =
        params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(param_refs.as_slice(), |row| {
        Ok((
            row.get::<_, i64>(0)?,            // id
            row.get::<_, String>(1)?,          // file_path
            row.get::<_, Option<String>>(2)?,  // title
            row.get::<_, Option<i32>>(3)?,     // year
            row.get::<_, Option<String>>(4)?,  // venue
            row.get::<_, Option<String>>(5)?,  // doi
            row.get::<_, Option<String>>(6)?,  // status
        ))
    })?;

    let mut paper_rows: Vec<PaperRow> = Vec::new();
    for row in rows {
        paper_rows.push(row?);
    }

    // 各 paper_id について関連テーブルから追加情報を取得
    let mut results: Vec<PaperResult> = Vec::new();
    for (id, file_path, title, year, venue, doi, status) in paper_rows {
        let authors = fetch_related_strings(
            conn,
            "SELECT name FROM authors WHERE paper_id = ?1 ORDER BY position",
            id,
        )?;
        let tags =
            fetch_related_strings(conn, "SELECT tag FROM tags WHERE paper_id = ?1", id)?;
        let fields_of_study = fetch_related_strings(
            conn,
            "SELECT field FROM fields_of_study WHERE paper_id = ?1",
            id,
        )?;
        let research_tasks = fetch_related_strings(
            conn,
            "SELECT task FROM research_tasks WHERE paper_id = ?1",
            id,
        )?;

        // Levenshtein 距離ベースの類似度スコアを計算する
        let similarity = compute_similarity(
            opts,
            title.as_deref(),
            venue.as_deref(),
            &authors,
            &tags,
            &research_tasks,
            &fields_of_study,
        );

        results.push(PaperResult {
            title,
            year,
            venue,
            doi,
            status,
            file_path,
            authors,
            tags,
            fields_of_study,
            research_tasks,
            similarity,
        });
    }

    // 類似度降順でソートする．同スコアの場合は年降順でタイブレーク
    results.sort_by(|a, b| {
        b.similarity
            .partial_cmp(&a.similarity)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let ya = a.year.unwrap_or(0);
                let yb = b.year.unwrap_or(0);
                yb.cmp(&ya)
            })
    });

    Ok(results)
}

/// フィルタ条件のテキストフィールドに対して正規化 Levenshtein 類似度を計算する．
/// 複数値を持つフィールド（author, keyword, field）は値の中の最大類似度を採用し，
/// 全クエリ対象フィールドの平均を論文全体のスコアとする．
/// テキストフィールドのクエリが無い場合は 1.0 を返す．
fn compute_similarity(
    opts: &FilterOptions,
    title: Option<&str>,
    venue: Option<&str>,
    authors: &[String],
    tags: &[String],
    research_tasks: &[String],
    fields_of_study: &[String],
) -> f64 {
    let mut scores: Vec<f64> = Vec::new();

    // --title
    if let Some(ref query) = opts.title {
        let q = query.to_lowercase();
        let val = title.unwrap_or("").to_lowercase();
        scores.push(normalized_levenshtein(&q, &val));
    }

    // --author: 全著者の中で最大の類似度を採用
    if let Some(ref query) = opts.author {
        let q = query.to_lowercase();
        let max_sim = authors
            .iter()
            .map(|a| normalized_levenshtein(&q, &a.to_lowercase()))
            .fold(0.0_f64, f64::max);
        scores.push(max_sim);
    }

    // --keyword: tags と research_tasks の両方から最大類似度を採用
    if let Some(ref query) = opts.keyword {
        let q = query.to_lowercase();
        let all_values: Vec<String> = tags
            .iter()
            .chain(research_tasks.iter())
            .map(|s| s.to_lowercase())
            .collect();
        let max_sim = all_values
            .iter()
            .map(|v| normalized_levenshtein(&q, v))
            .fold(0.0_f64, f64::max);
        scores.push(max_sim);
    }

    // --field: 全分野の中で最大の類似度を採用
    if let Some(ref query) = opts.field {
        let q = query.to_lowercase();
        let max_sim = fields_of_study
            .iter()
            .map(|f| normalized_levenshtein(&q, &f.to_lowercase()))
            .fold(0.0_f64, f64::max);
        scores.push(max_sim);
    }

    // --venue
    if let Some(ref query) = opts.venue {
        let q = query.to_lowercase();
        let val = venue.unwrap_or("").to_lowercase();
        scores.push(normalized_levenshtein(&q, &val));
    }

    if scores.is_empty() {
        1.0
    } else {
        scores.iter().sum::<f64>() / scores.len() as f64
    }
}

/// 関連テーブルから文字列のリストを取得するヘルパー．
fn fetch_related_strings(conn: &Connection, sql: &str, paper_id: i64) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([paper_id], |row| row.get::<_, String>(0))?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// Stats
// ---------------------------------------------------------------------------

/// キャッシュ DB の統計情報を取得する．
pub fn get_stats(conn: &Connection) -> Result<Stats> {
    // 総件数
    let total: usize = conn.query_row("SELECT COUNT(*) FROM papers", [], |row| {
        row.get::<_, usize>(0)
    })?;

    // ステータス別
    let mut stmt = conn.prepare(
        "SELECT status, COUNT(*) FROM papers GROUP BY status ORDER BY COUNT(*) DESC",
    )?;
    let by_status: Vec<(String, usize)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?
                    .unwrap_or_else(|| "(none)".to_string()),
                row.get::<_, usize>(1)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    // 年別
    let mut stmt = conn.prepare(
        "SELECT year, COUNT(*) FROM papers WHERE year IS NOT NULL GROUP BY year ORDER BY year DESC",
    )?;
    let by_year: Vec<(i32, usize)> = stmt
        .query_map([], |row| Ok((row.get::<_, i32>(0)?, row.get::<_, usize>(1)?)))?
        .filter_map(|r| r.ok())
        .collect();

    // 分野別 (top 20)
    let mut stmt = conn.prepare(
        "SELECT field, COUNT(*) FROM fields_of_study GROUP BY field ORDER BY COUNT(*) DESC LIMIT 20",
    )?;
    let by_field: Vec<(String, usize)> = stmt
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, usize>(1)?)))?
        .filter_map(|r| r.ok())
        .collect();

    // 著者別 (top 20)
    let mut stmt = conn.prepare(
        "SELECT name, COUNT(*) FROM authors GROUP BY name ORDER BY COUNT(*) DESC LIMIT 20",
    )?;
    let top_authors: Vec<(String, usize)> = stmt
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, usize>(1)?)))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(Stats {
        total,
        by_status,
        by_year,
        by_field,
        top_authors,
    })
}
