# GUI 設計境界

**Languages:** [English](../en/DESIGN.md) | [简体中文](../zh-CN/DESIGN.md) | [日本語](DESIGN.md)

## 情報アーキテクチャ

- **初回言語選択**（`views/locale_setup.rs`）: 言語未設定時に表示
- **ロック画面**（`views/locked.rs`）: カード式の開く/作成、最近、パスワードモード
- **ブラウザ**（`views/browser.rs`）: サイドバー + パンくず + ファイル一覧
- **設定**（`views/settings.rs`）: 自動ロック、防截屏、WinFsp リンク、テーマ、**言語**

テーマ: 黒曜石/暗金（ダーク）、真珠白/石板灰（ライト）。Lucide アイコンフォント同梱。

## 実装済み

- フォルダーインポート時にソース名を保持
- 再帰ディレクトリ抽出、競合事前チェック、`ExtractMountReadOnly`（現在マウント中のパスへの抽出拒否）
- 削除確認ダイアログ（単一/一括）、抽出・シンボリックリンクの説明ダイアログ（「次回から表示しない」可）
- Ctrl/Shift 複数選択、一括削除/抽出（リネーム・外部で開くは単一選択のみ）
- マウント/アンマウント（`gui-mount`）、ファイル/フォルダーシンボリックリンク
- 防截屏: Windows/macOS は winit `set_content_protected`、Linux はベストエフォート（環境別説明）
- Noto Sans SC 子セットによる CJK 表示

## 境界

- Linux FUSE と Windows WinFsp のマウントは書き込み可能（クローズ時にファイル全体を書き戻す）。macOS は WebDAV ブリッジで、Finder の挙動は Mac 上で未検証。一括抽出は既存ディレクトリとマージしない。
- 防截屏はセキュリティ境界ではない。macOS 15+ の ScreenCaptureKit はキャプチャできる場合あり。

## ビルド

```bash
cargo run --features gui-mount --bin sesame-gui
```
