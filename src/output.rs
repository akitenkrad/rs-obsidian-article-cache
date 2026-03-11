use anyhow::Result;
use comfy_table::{Cell, Table};
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::Writer;

use crate::models::PaperResult;

/// テーブル形式で出力する．
pub fn print_table(papers: &[PaperResult]) {
    if papers.is_empty() {
        println!("No papers found.");
        return;
    }

    let mut table = Table::new();
    table.set_header(vec!["Title", "Year", "Authors", "Venue", "Status"]);

    for paper in papers {
        let title = truncate(
            paper.title.as_deref().unwrap_or("(untitled)"),
            50,
        );
        let year = paper
            .year
            .map(|y| y.to_string())
            .unwrap_or_default();
        let authors = format_authors(&paper.authors, 3);
        let venue = paper.venue.as_deref().unwrap_or("").to_string();
        let status = paper.status.as_deref().unwrap_or("").to_string();

        table.add_row(vec![
            Cell::new(title),
            Cell::new(year),
            Cell::new(authors),
            Cell::new(venue),
            Cell::new(status),
        ]);
    }

    println!("{table}");
}

/// JSON 形式で出力する．
pub fn print_json(papers: &[PaperResult]) -> Result<()> {
    let json = serde_json::to_string_pretty(papers)?;
    println!("{json}");
    Ok(())
}

/// XML 形式で出力する．
pub fn print_xml(papers: &[PaperResult]) -> Result<()> {
    let mut writer = Writer::new_with_indent(Vec::new(), b' ', 2);

    // XML declaration
    writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))?;

    // <papers>
    writer.write_event(Event::Start(BytesStart::new("papers")))?;

    for paper in papers {
        writer.write_event(Event::Start(BytesStart::new("paper")))?;

        write_text_element(&mut writer, "title", paper.title.as_deref().unwrap_or(""))?;
        write_text_element(
            &mut writer,
            "year",
            &paper.year.map(|y| y.to_string()).unwrap_or_default(),
        )?;
        write_text_element(&mut writer, "venue", paper.venue.as_deref().unwrap_or(""))?;
        write_text_element(&mut writer, "doi", paper.doi.as_deref().unwrap_or(""))?;
        write_text_element(&mut writer, "status", paper.status.as_deref().unwrap_or(""))?;
        write_text_element(&mut writer, "file_path", &paper.file_path)?;

        // <authors>
        write_list_elements(&mut writer, "authors", "author", &paper.authors)?;
        // <tags>
        write_list_elements(&mut writer, "tags", "tag", &paper.tags)?;
        // <fields_of_study>
        write_list_elements(&mut writer, "fields_of_study", "field", &paper.fields_of_study)?;
        // <research_tasks>
        write_list_elements(&mut writer, "research_tasks", "task", &paper.research_tasks)?;

        writer.write_event(Event::End(BytesEnd::new("paper")))?;
    }

    // </papers>
    writer.write_event(Event::End(BytesEnd::new("papers")))?;

    let xml = String::from_utf8(writer.into_inner())?;
    println!("{xml}");
    Ok(())
}

/// ファイルパスのみを出力する．
pub fn print_paths(papers: &[PaperResult]) {
    for paper in papers {
        println!("{}", paper.file_path);
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// 文字列を指定文字数で切り詰める．
fn truncate(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        s.to_string()
    } else {
        let truncated: String = chars[..max - 3].iter().collect();
        format!("{}...", truncated)
    }
}

/// 著者リストを整形する．max 名まで表示し，超過分は "et al." を付加する．
fn format_authors(authors: &[String], max: usize) -> String {
    if authors.is_empty() {
        return String::new();
    }
    if authors.len() <= max {
        authors.join(", ")
    } else {
        let shown: Vec<&str> = authors[..max].iter().map(|s| s.as_str()).collect();
        format!("{} et al.", shown.join(", "))
    }
}

/// XML のテキスト要素を書き込むヘルパー．
fn write_text_element(writer: &mut Writer<Vec<u8>>, tag: &str, text: &str) -> Result<()> {
    writer.write_event(Event::Start(BytesStart::new(tag)))?;
    writer.write_event(Event::Text(BytesText::new(text)))?;
    writer.write_event(Event::End(BytesEnd::new(tag)))?;
    Ok(())
}

/// XML のリスト要素を書き込むヘルパー．
fn write_list_elements(
    writer: &mut Writer<Vec<u8>>,
    wrapper: &str,
    item: &str,
    items: &[String],
) -> Result<()> {
    writer.write_event(Event::Start(BytesStart::new(wrapper)))?;
    for s in items {
        write_text_element(writer, item, s)?;
    }
    writer.write_event(Event::End(BytesEnd::new(wrapper)))?;
    Ok(())
}
