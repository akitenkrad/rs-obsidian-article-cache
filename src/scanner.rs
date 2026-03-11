use std::path::{Path, PathBuf};

use anyhow::Result;
use walkdir::WalkDir;

/// 除外ディレクトリ名
const EXCLUDED_DIRS: &[&str] = &[".obsidian", ".git", "_templates", ".trash"];

/// Vault を再帰的にスキャンし，`.md` ファイルのパス一覧を返す．
/// `.obsidian/`, `.git/`, `_templates/`, `.trash/` は除外する．
pub fn scan_vault(vault_path: &Path) -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = WalkDir::new(vault_path)
        .into_iter()
        .filter_entry(|entry| {
            // ディレクトリの場合，除外リストに含まれていたらスキップ
            if entry.file_type().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    return !EXCLUDED_DIRS.contains(&name);
                }
            }
            true
        })
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().is_file()
                && e.path()
                    .extension()
                    .map(|ext| ext == "md")
                    .unwrap_or(false)
        })
        .map(|e| e.into_path())
        .collect();

    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_scan_vault_excludes_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();

        // 通常の .md ファイル
        fs::write(base.join("note.md"), "# Note").unwrap();

        // サブディレクトリ内の .md
        fs::create_dir_all(base.join("sub")).unwrap();
        fs::write(base.join("sub/paper.md"), "# Paper").unwrap();

        // 除外ディレクトリ内の .md
        fs::create_dir_all(base.join(".obsidian")).unwrap();
        fs::write(base.join(".obsidian/config.md"), "config").unwrap();

        fs::create_dir_all(base.join(".git")).unwrap();
        fs::write(base.join(".git/info.md"), "info").unwrap();

        // .txt ファイル（対象外）
        fs::write(base.join("readme.txt"), "text").unwrap();

        let result = scan_vault(base).unwrap();
        let names: Vec<&str> = result
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap())
            .collect();

        assert!(names.contains(&"note.md"));
        assert!(names.contains(&"paper.md"));
        assert!(!names.contains(&"config.md"));
        assert!(!names.contains(&"info.md"));
        assert!(!names.contains(&"readme.txt"));
        assert_eq!(result.len(), 2);
    }
}
