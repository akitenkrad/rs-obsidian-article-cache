use std::path::PathBuf;
use std::process::Command;

/// テスト用バイナリのパスを取得する．
fn binary_path() -> PathBuf {
    // `cargo test` 実行時，ビルド済みバイナリは target/debug/ にある
    let mut path = PathBuf::from(env!("CARGO_BIN_EXE_obsidian-paper-cache"));
    // env! が使えない場合のフォールバック
    if !path.exists() {
        path = PathBuf::from("target/debug/obsidian-paper-cache");
    }
    path
}

/// fixtures ディレクトリのパスを取得する．
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// 一時ディレクトリにフィクスチャをコピーし，build を実行する．
/// (db_path, tmp_dir) を返す．tmp_dir は RAII で削除される．
fn setup_and_build(force: bool) -> (PathBuf, tempfile::TempDir, tempfile::TempDir) {
    let vault_dir = tempfile::tempdir().unwrap();
    let db_dir = tempfile::tempdir().unwrap();

    // フィクスチャを一時 vault にコピー
    let fixtures = fixtures_dir();
    for entry in std::fs::read_dir(&fixtures).unwrap() {
        let entry = entry.unwrap();
        let dest = vault_dir.path().join(entry.file_name());
        std::fs::copy(entry.path(), &dest).unwrap();
    }

    let db_path = db_dir.path().join("test.db");

    let mut cmd = Command::new(binary_path());
    cmd.arg("build")
        .arg("--vault")
        .arg(vault_dir.path())
        .arg("--db")
        .arg(&db_path);

    if force {
        cmd.arg("--force");
    }

    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    (db_path, vault_dir, db_dir)
}

#[test]
fn test_build_paper_count() {
    let (db_path, _vault, _db) = setup_and_build(false);

    // build 出力から "Added" の件数を確認
    // 直接 DB を確認: stats コマンドで total を見る
    let output = Command::new(binary_path())
        .arg("stats")
        .arg("--db")
        .arg(&db_path)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // complete_paper, minimal_paper, na_fields の3件（no_tag はスキップ）
    assert!(
        stdout.contains("Total papers: 3"),
        "Expected 3 papers, got: {}",
        stdout
    );
}

#[test]
fn test_filter_by_title() {
    let (db_path, _vault, _db) = setup_and_build(false);

    let output = Command::new(binary_path())
        .arg("filter")
        .arg("--title")
        .arg("Complete Test Paper")
        .arg("--format")
        .arg("json")
        .arg("--db")
        .arg(&db_path)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("A Complete Test Paper on Machine Learning"),
        "Title filter failed: {}",
        stdout
    );

    // パースして件数確認
    let papers: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    assert_eq!(papers.len(), 1);
}

#[test]
fn test_filter_by_author() {
    let (db_path, _vault, _db) = setup_and_build(false);

    let output = Command::new(binary_path())
        .arg("filter")
        .arg("--author")
        .arg("Alice Smith")
        .arg("--format")
        .arg("json")
        .arg("--db")
        .arg(&db_path)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let papers: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    assert_eq!(papers.len(), 1);
    assert_eq!(
        papers[0]["title"].as_str().unwrap(),
        "A Complete Test Paper on Machine Learning"
    );
}

#[test]
fn test_filter_by_year_range() {
    let (db_path, _vault, _db) = setup_and_build(false);

    // year_from=2020, year_to=2023 → complete_paper (2023) + minimal_paper (2020)
    let output = Command::new(binary_path())
        .arg("filter")
        .arg("--year-from")
        .arg("2020")
        .arg("--year-to")
        .arg("2023")
        .arg("--format")
        .arg("json")
        .arg("--db")
        .arg(&db_path)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let papers: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        papers.len(),
        2,
        "Expected 2 papers in 2020-2023 range, got: {}",
        stdout
    );
}

#[test]
fn test_filter_by_keyword() {
    let (db_path, _vault, _db) = setup_and_build(false);

    // keyword "Classification" matches complete_paper's research_tasks
    let output = Command::new(binary_path())
        .arg("filter")
        .arg("--keyword")
        .arg("Classification")
        .arg("--format")
        .arg("json")
        .arg("--db")
        .arg(&db_path)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let papers: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    assert_eq!(papers.len(), 1);
    assert!(stdout.contains("A Complete Test Paper on Machine Learning"));
}

#[test]
fn test_filter_by_status() {
    let (db_path, _vault, _db) = setup_and_build(false);

    let output = Command::new(binary_path())
        .arg("filter")
        .arg("--status")
        .arg("done")
        .arg("--format")
        .arg("json")
        .arg("--db")
        .arg(&db_path)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let papers: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    assert_eq!(papers.len(), 1);
    assert!(stdout.contains("A Complete Test Paper on Machine Learning"));
}

#[test]
fn test_stats_total_count() {
    let (db_path, _vault, _db) = setup_and_build(false);

    let output = Command::new(binary_path())
        .arg("stats")
        .arg("--db")
        .arg(&db_path)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Total papers: 3"));
    // ステータス別の確認
    assert!(stdout.contains("done"));
    assert!(stdout.contains("to read"));
}

#[test]
fn test_incremental_build_skips_unchanged() {
    let vault_dir = tempfile::tempdir().unwrap();
    let db_dir = tempfile::tempdir().unwrap();

    // フィクスチャをコピー
    let fixtures = fixtures_dir();
    for entry in std::fs::read_dir(&fixtures).unwrap() {
        let entry = entry.unwrap();
        let dest = vault_dir.path().join(entry.file_name());
        std::fs::copy(entry.path(), &dest).unwrap();
    }

    let db_path = db_dir.path().join("test.db");

    // 1回目の build
    let output1 = Command::new(binary_path())
        .arg("build")
        .arg("--vault")
        .arg(vault_dir.path())
        .arg("--db")
        .arg(&db_path)
        .output()
        .unwrap();
    assert!(output1.status.success());
    let stdout1 = String::from_utf8_lossy(&output1.stdout);
    assert!(
        stdout1.contains("Added               : 3"),
        "First build should add 3 papers: {}",
        stdout1
    );

    // 2回目の build（差分なし → 全件スキップ）
    let output2 = Command::new(binary_path())
        .arg("build")
        .arg("--vault")
        .arg(vault_dir.path())
        .arg("--db")
        .arg(&db_path)
        .output()
        .unwrap();
    assert!(output2.status.success());
    let stdout2 = String::from_utf8_lossy(&output2.stdout);
    assert!(
        stdout2.contains("Skipped (unchanged) : 3"),
        "Second build should skip 3 papers: {}",
        stdout2
    );
    assert!(
        stdout2.contains("Added               : 0"),
        "Second build should add 0 papers: {}",
        stdout2
    );
}

#[test]
fn test_force_rebuild() {
    let vault_dir = tempfile::tempdir().unwrap();
    let db_dir = tempfile::tempdir().unwrap();

    // フィクスチャをコピー
    let fixtures = fixtures_dir();
    for entry in std::fs::read_dir(&fixtures).unwrap() {
        let entry = entry.unwrap();
        let dest = vault_dir.path().join(entry.file_name());
        std::fs::copy(entry.path(), &dest).unwrap();
    }

    let db_path = db_dir.path().join("test.db");

    // 1回目の build
    let output1 = Command::new(binary_path())
        .arg("build")
        .arg("--vault")
        .arg(vault_dir.path())
        .arg("--db")
        .arg(&db_path)
        .output()
        .unwrap();
    assert!(output1.status.success());

    // --force で再構築
    let output2 = Command::new(binary_path())
        .arg("build")
        .arg("--force")
        .arg("--vault")
        .arg(vault_dir.path())
        .arg("--db")
        .arg(&db_path)
        .output()
        .unwrap();
    assert!(output2.status.success());
    let stdout2 = String::from_utf8_lossy(&output2.stdout);
    // force 時は全件再追加
    assert!(
        stdout2.contains("Added               : 3"),
        "Force rebuild should add 3 papers: {}",
        stdout2
    );
}
