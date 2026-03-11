mod cli;
mod db;
mod models;
mod output;
mod parser;
mod scanner;

use std::collections::HashSet;
use std::fs;

use anyhow::Result;
use chrono::{DateTime, Utc};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};

use cli::{Cli, Commands, OutputFormat, expand_tilde};

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Build { vault, db, force } => {
            let db_path = expand_tilde(&db);
            cmd_build(&vault, &db_path, force)
        }
        Commands::Filter {
            title,
            author,
            year,
            year_from,
            year_to,
            keyword,
            field,
            venue,
            status,
            limit,
            format,
            db,
        } => {
            let db_path = expand_tilde(&db);
            cmd_filter(
                &db_path, title, author, year, year_from, year_to, keyword, field, venue,
                status, limit, format,
            )
        }
        Commands::Stats { db } => {
            let db_path = expand_tilde(&db);
            cmd_stats(&db_path)
        }
    }
}

fn cmd_build(
    vault: &std::path::Path,
    db_path: &std::path::Path,
    force: bool,
) -> Result<()> {
    let conn = db::open_db(db_path)?;

    // --force: 全テーブル DROP して再初期化
    if force {
        db::drop_all_tables(&conn)?;
        eprintln!("Dropped all tables (--force)");
    }

    db::initialize_schema(&conn)?;

    // Vault スキャン
    let files = scanner::scan_vault(vault)?;
    let total = files.len();
    eprintln!("Found {} markdown files in {}", total, vault.display());

    // プログレスバー
    let pb = ProgressBar::new(total as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("##-"),
    );

    let mut added = 0usize;
    let mut updated = 0usize;
    let mut skipped = 0usize;
    let mut parse_skipped = 0usize; // 学術論文タグなし等
    let mut errors = 0usize;

    // 既存パスの収集（delete_removed_papers 用）
    let existing_paths: HashSet<String> = files
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    for file_path in &files {
        let path_str = file_path.to_string_lossy().to_string();

        // ファイルの modified_at を取得
        let modified_at = match fs::metadata(file_path) {
            Ok(meta) => {
                if let Ok(modified) = meta.modified() {
                    let dt: DateTime<Utc> = modified.into();
                    dt.to_rfc3339()
                } else {
                    String::new()
                }
            }
            Err(_) => {
                errors += 1;
                pb.inc(1);
                continue;
            }
        };

        // 差分チェック: modified_at が同じならスキップ
        let cached = db::get_cached_modified_at(&conn, &path_str)?;
        if let Some(ref cached_modified) = cached {
            if *cached_modified == modified_at {
                skipped += 1;
                pb.inc(1);
                continue;
            }
        }
        let is_update = cached.is_some();

        // ファイル読み込み + パース
        let content = match fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("\nWarning: Failed to read {}: {}", path_str, e);
                errors += 1;
                pb.inc(1);
                continue;
            }
        };

        match parser::parse_paper(file_path, &content, &modified_at) {
            Ok(Some(paper)) => {
                if let Err(e) = db::upsert_paper(&conn, &paper) {
                    eprintln!("\nWarning: Failed to upsert {}: {}", path_str, e);
                    errors += 1;
                } else if is_update {
                    updated += 1;
                } else {
                    added += 1;
                }
            }
            Ok(None) => {
                // 学術論文タグなし等 — スキップ
                parse_skipped += 1;
            }
            Err(e) => {
                eprintln!("\nWarning: Failed to parse {}: {}", path_str, e);
                errors += 1;
            }
        }

        pb.inc(1);
    }

    pb.finish_with_message("done");

    // Vault から消えたファイルを DB から削除
    let deleted = db::delete_removed_papers(&conn, &existing_paths)?;

    // サマリ出力
    println!();
    println!("=== Build Summary ===");
    println!("  Total files scanned : {}", total);
    println!("  Added               : {}", added);
    println!("  Updated             : {}", updated);
    println!("  Skipped (unchanged) : {}", skipped);
    println!("  Skipped (non-paper) : {}", parse_skipped);
    println!("  Deleted (removed)   : {}", deleted);
    if errors > 0 {
        println!("  Errors              : {}", errors);
    }
    println!("  Database            : {}", db_path.display());

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_filter(
    db_path: &std::path::Path,
    title: Option<String>,
    author: Option<String>,
    year: Option<i32>,
    year_from: Option<i32>,
    year_to: Option<i32>,
    keyword: Option<String>,
    field: Option<String>,
    venue: Option<String>,
    status: Option<String>,
    limit: usize,
    format: OutputFormat,
) -> Result<()> {
    let conn = db::open_db(db_path)?;
    db::initialize_schema(&conn)?;

    let opts = db::FilterOptions {
        title,
        author,
        year,
        year_from,
        year_to,
        keyword,
        field,
        venue,
        status,
        limit,
    };

    let papers = db::filter_papers(&conn, &opts)?;

    match format {
        OutputFormat::Table => output::print_table(&papers),
        OutputFormat::Json => output::print_json(&papers)?,
        OutputFormat::Xml => output::print_xml(&papers)?,
        OutputFormat::Paths => output::print_paths(&papers),
    }

    Ok(())
}

fn cmd_stats(db_path: &std::path::Path) -> Result<()> {
    let conn = db::open_db(db_path)?;
    db::initialize_schema(&conn)?;

    let stats = db::get_stats(&conn)?;

    println!("=== Paper Cache Statistics ===");
    println!();
    println!("Total papers: {}", stats.total);

    // By Status
    println!();
    println!("--- By Status ---");
    if stats.by_status.is_empty() {
        println!("  (no data)");
    } else {
        let max_label = stats.by_status.iter().map(|(s, _)| s.len()).max().unwrap_or(0);
        for (status, count) in &stats.by_status {
            println!("  {:<width$}  {:>6}", status, count, width = max_label);
        }
    }

    // By Year (top 20)
    println!();
    println!("--- By Year (top 20) ---");
    let year_entries: Vec<_> = stats.by_year.iter().take(20).collect();
    if year_entries.is_empty() {
        println!("  (no data)");
    } else {
        for (year, count) in &year_entries {
            println!("  {}  {:>6}", year, count);
        }
    }

    // By Field (top 20)
    println!();
    println!("--- By Field (top 20) ---");
    if stats.by_field.is_empty() {
        println!("  (no data)");
    } else {
        let max_label = stats.by_field.iter().map(|(s, _)| s.len()).max().unwrap_or(0);
        for (field, count) in &stats.by_field {
            println!("  {:<width$}  {:>6}", field, count, width = max_label);
        }
    }

    // Top Authors (top 20)
    println!();
    println!("--- Top Authors (top 20) ---");
    if stats.top_authors.is_empty() {
        println!("  (no data)");
    } else {
        let max_label = stats.top_authors.iter().map(|(s, _)| s.len()).max().unwrap_or(0);
        for (name, count) in &stats.top_authors {
            println!("  {:<width$}  {:>6}", name, count, width = max_label);
        }
    }

    Ok(())
}
