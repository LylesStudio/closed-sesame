# ClosedSesame アーキテクチャ

**Languages:** [English](../en/ARCHITECTURE.md) | [简体中文](../zh-CN/ARCHITECTURE.md) | [日本語](ARCHITECTURE.md)

コンテナ形式とユースケースはフロントエンドやマウント実装に依存しません。

## モジュール境界

```text
CLI / GUI / recover → app → container → crypto / keymgmt
mount → SharedVault → FUSE（読み書き）/ WebDAV ブリッジ / WinFsp（読み書き）
```

- `app/vault.rs`: 共有抽出ロジック、`ensure_extract_destination` による競合事前チェック
- `app/symlink.rs`: ファイルと**ディレクトリ**のシンボリックリンク登録表
- `gui/views/`: ロック/ブラウザ/設定/`locale_setup`、`gui/dialog.rs`（削除/抽出/symlink/最近の解除モーダル）
- `gui/anti_capture.rs`: `ViewportCommand::ContentProtected`、Linux 環境検出
- `i18n`: `locales/*.yml` による集中翻訳（`rust-i18n`）
- `mount_hints.rs`: マウントランタイムの案内文

## 抽出と競合

- 通常ファイル/ディレクトリの上書きは `ExtractConflict` で拒否。現在マウント中のパスへの抽出は `ExtractMountReadOnly` で拒否し、出力をマウントビューの外に置く。
- ディレクトリ抽出は子ファイルを収集し、全宛先を事前検証（一括は all-or-nothing）。
- `recovery::extract_all` は GUI/CLI と同じ helper を使用。

## シンボリックリンク

- マウント中かつユーザーが有効化した場合のみ。
- ファイル: `move_with_symlink`、フォルダー: `move_dir_with_symlink`（ソースツリー削除後にディレクトリリンクを配置）。

## フロントエンド

- CLI: 全コマンド、`add --recursive`（`-n` 省略時はソース名をデフォルト）
- GUI（`gui-mount`）: サイドバー + パンくず、複数選択、確認ダイアログ付き一括削除/抽出、初回言語選択、防截屏（Windows/macOS は winit）
- `sesame-recover`: 読み取り専用 verify/list/extract、GUI と同じ競合ルール

## 検証状態

CI は WinFsp 未インストールでも `cli,mount,gui-mount,recover` をビルド検証します。Linux は CI が `/dev/fuse` を提供するとき `fuse_mount_tests` を実行します。Windows の実機マウントとディレクトリシンボリックリンクのテストは、そのマシンに WinFsp が必要です。macOS は組み込み loopback WebDAV ブリッジでマウントし、`webdav_tests` がプロトコルをカバーします。旧 macFUSE/FSKit 経路は削除済みです。`mount_vault` は macOS で失敗し、`sesame mount --webdav` を案内します。Finder の実機動作は未検証です。
