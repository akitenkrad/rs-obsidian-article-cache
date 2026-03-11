use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "obsidian-paper-cache")]
#[command(about = "Obsidian Vault の学術論文メタデータをキャッシュ・検索する CLI ツール")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Obsidian Vault をスキャンしてキャッシュを構築/更新
    Build {
        /// Obsidian Vault のパス
        #[arg(long, default_value = "~/Documents/Obsidian")]
        vault: String,

        /// SQLite データベースのパス
        #[arg(long, default_value = "~/.cache/obsidian-paper-cache/papers.db")]
        db: String,

        /// 既存キャッシュを破棄して全件再構築
        #[arg(long)]
        force: bool,
    },

    /// キャッシュから論文を検索・フィルタ
    Filter {
        /// タイトルで部分一致検索
        #[arg(long)]
        title: Option<String>,

        /// 著者名で部分一致検索
        #[arg(long)]
        author: Option<String>,

        /// 発行年で完全一致
        #[arg(long)]
        year: Option<i32>,

        /// 発行年の下限
        #[arg(long)]
        year_from: Option<i32>,

        /// 発行年の上限
        #[arg(long)]
        year_to: Option<i32>,

        /// 研究タスク/タグで部分一致検索
        #[arg(long)]
        keyword: Option<String>,

        /// 研究分野で部分一致検索
        #[arg(long)]
        field: Option<String>,

        /// 掲載誌/学会で部分一致検索
        #[arg(long)]
        venue: Option<String>,

        /// 読了ステータスでフィルタ
        #[arg(long)]
        status: Option<String>,

        /// 結果の最大件数
        #[arg(long, default_value_t = 20)]
        limit: usize,

        /// 出力形式
        #[arg(long, value_enum, default_value_t = OutputFormat::Table)]
        format: OutputFormat,

        /// SQLite データベースのパス
        #[arg(long, default_value = "~/.cache/obsidian-paper-cache/papers.db")]
        db: String,
    },

    /// キャッシュの統計情報を表示
    Stats {
        /// SQLite データベースのパス
        #[arg(long, default_value = "~/.cache/obsidian-paper-cache/papers.db")]
        db: String,
    },
}

#[derive(Debug, Clone, ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
    Xml,
    Paths,
}

/// `~` をホームディレクトリに展開する
pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs_home() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}
