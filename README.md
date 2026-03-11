# obsidian-paper-cache

Obsidian Vault 内の学術論文メタデータを SQLite にキャッシュし，高速に検索・フィルタする Rust CLI ツール．

Claude Code のスキルとして利用することで，学術論文の検索コストを削減できます．

## Features

- Obsidian の YAML frontmatter + 書誌情報テーブルを自動解析
- `学術論文` タグ付きファイルを自動抽出
- タイトル，著者，発行年，研究分野，キーワード，学会，読了ステータスでフィルタ
- 差分更新対応（変更ファイルのみ再パース）
- 4 種類の出力形式（table / json / xml / paths）

## Installation

```bash
cargo install --path .
```

## Usage

### キャッシュの構築

```bash
# デフォルト設定で構築（差分更新）
obsidian-paper-cache build

# Vault パスを指定
obsidian-paper-cache build --vault /path/to/obsidian/vault

# 全件再構築
obsidian-paper-cache build --force
```

```
$ obsidian-paper-cache build
Scanning vault: /Users/akitenkrad/Documents/Obsidian
Found 3355 markdown files
Building cache... ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ 3355/3355

Build complete:
  Added:   1191
  Updated: 0
  Skipped: 2164
  Deleted: 0
  Errors:  0
```

### 論文の検索・フィルタ

```bash
# タイトルで検索
obsidian-paper-cache filter --title "intrusion detection"

# 著者で検索
obsidian-paper-cache filter --author "Zaki"

# 年の範囲で絞り込み
obsidian-paper-cache filter --year-from 2020 --year-to 2024

# キーワード（研究タスク/タグ）で検索
obsidian-paper-cache filter --keyword "anomaly"

# 研究分野で検索
obsidian-paper-cache filter --field "Computer Security"

# 学会で検索
obsidian-paper-cache filter --venue "NeurIPS"

# 読了ステータスでフィルタ
obsidian-paper-cache filter --status "to read" --limit 10

# 条件の組み合わせ（AND 結合）
obsidian-paper-cache filter --field "Computer Security" --year-from 2020 --status "to read"
```

複数の条件を指定した場合は AND 結合で絞り込みます．

### 出力形式

```bash
# テーブル形式（デフォルト）
obsidian-paper-cache filter --title "intrusion" --format table
```

```
+----------------------------------------------------+------+---------+-------+---------+
| Title                                              | Year | Authors | Venue | Status  |
+=======================================================================================+
| A Deep Learning-Machine Learning Approach for A... | 2025 |         |       | to read |
|----------------------------------------------------+------+---------+-------+---------|
| A lightweight intrusion detection system using ... | 2025 |         |       | to read |
+----------------------------------------------------+------+---------+-------+---------+
```

```bash
# JSON 形式
obsidian-paper-cache filter --author "Zaki" --format json
```

```json
[
  {
    "title": "ADMIT: Anomaly-based Data Mining for Intrusions",
    "year": 2002,
    "venue": "Proceedings of the Eighth ACM SIGKDD...",
    "doi": "10.1145/775047.775103",
    "status": "to read",
    "file_path": "/Users/.../ADMIT - Anomaly-based Data Mining for Intrusions.md",
    "authors": ["Karlton Sequeira", "Mohammed J. Zaki"],
    "tags": ["学術論文", "IntrusionDetection"],
    "fields_of_study": ["Computer Security"],
    "research_tasks": ["Intrusion Detection", "Anomaly Detection"]
  }
]
```

```bash
# XML 形式
obsidian-paper-cache filter --author "Zaki" --format xml
```

```xml
<?xml version="1.0" encoding="UTF-8"?>
<papers>
  <paper>
    <title>ADMIT: Anomaly-based Data Mining for Intrusions</title>
    <year>2002</year>
    <venue>Proceedings of the Eighth ACM SIGKDD...</venue>
    <authors>
      <author>Karlton Sequeira</author>
      <author>Mohammed J. Zaki</author>
    </authors>
    ...
  </paper>
</papers>
```

```bash
# パスのみ出力（Claude Code 連携用）
obsidian-paper-cache filter --keyword "anomaly" --format paths
```

```
/Users/.../ADMIT - Anomaly-based Data Mining for Intrusions.md
/Users/.../Network Anomaly Detection - A Survey.md
```

### 統計情報

```bash
obsidian-paper-cache stats
```

```
=== Paper Cache Statistics ===

Total papers: 1191

--- By Status ---
  to read      1133
  read           25
  completed      18
  ...

--- By Year (top 20) ---
  2025      68
  2024      89
  2023      88
  ...

--- By Field (top 20) ---
  Social Simulation               767
  Computer Security               280
  ...

--- Top Authors (top 20) ---
  Wenke Lee                10
  Salvatore J. Stolfo       8
  ...
```

## Obsidian ファイル形式

本ツールは以下の形式の Markdown ファイルを解析します．

### YAML Frontmatter

```yaml
---
tags:
  - 学術論文          # 必須: このタグがあるファイルのみ対象
  - IntrusionDetection  # 追加タグ（任意）
ac-paper-status: to read  # 読了ステータス
year: 2002
field-of-study:
  - Computer Security
created: 2026-02-22 12:00:00
---
```

### 書誌情報テーブル

```markdown
## 書誌情報

| 項目 | 内容 |
|------|------|
| **タイトル** | Paper Title Here |
| **著者** | Author A, Author B |
| **発行年** | 2023 |
| **掲載誌/学会** | Conference Name |
| **DOI** | 10.1234/example |
| **arXiv ID** | 2301.12345 |
| **研究タスク** | Task A, Task B |
| **被引用数** | 42 |
| **オープンアクセス** | Yes |
```

## Database

キャッシュは SQLite データベースとして保存されます．

- デフォルトパス: `~/.cache/obsidian-paper-cache/papers.db`
- `--db` オプションで変更可能

### スキーマ

| テーブル | 内容 |
|---------|------|
| `papers` | 論文の基本メタデータ（タイトル，年，DOI 等） |
| `authors` | 著者（論文との多対多） |
| `tags` | タグ（学術論文，IntrusionDetection 等） |
| `fields_of_study` | 研究分野 |
| `research_tasks` | 研究タスク |

## Development

```bash
# ビルド
cargo build

# テスト
cargo test

# Lint
cargo clippy

# リリースビルド
cargo build --release
```

## License

MIT
