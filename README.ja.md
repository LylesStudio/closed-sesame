# ClosedSesame 🔒

**言語:** [English](README.md) | [简体中文](README.zh-CN.md) | [日本語](README.ja.md)

ClosedSesame は Rust で書かれた暗号化ファイルコンテナの実験プロジェクトです。ファイルは AES-256-GCM、Argon2id/HKDF、ページ単位の認証付き暗号を用いる `.sesame` コンテナに格納されます。

プロジェクトはまだ初期段階です。唯一のコピーをコンテナ内だけに置かないでください。

## デモ動画

`v0.1.0` リリースに完成したプロモーション動画を収録しています:

- [English promo video (MP4)](https://github.com/Silver-Aurora/closed-sesame/releases/download/v0.1.0/closed-sesame-promo-en.mp4)
- [中文宣传片（MP4）](https://github.com/Silver-Aurora/closed-sesame/releases/download/v0.1.0/closed-sesame-promo-zh.mp4)

## 国際化（i18n）

実行時 UI と CLI は **英語**、**簡体字中国語**、**日本語** に対応しています。

- CLI / recover: `--lang en|zh-CN|ja` または `CLOSED_SESAME_LANG`
- GUI: 初回起動時の言語選択（未設定時）; 設定 → 言語（OS 設定ディレクトリに保存）
- 翻訳ファイル: [`locales/`](locales/) — 詳細は [docs/ja/CONTRIBUTING-i18n.md](docs/ja/CONTRIBUTING-i18n.md)

## 現在の機能

状態の凡例：**実装済み**はコードパスが存在すること、**ビルド検証**は CI でビルド/テスト済み、**実機検証**は実際のマウントとファイルアクセスを確認済み。**Roadmap** は未実装。

- コンテナ
  - **利用可**: 自己記述 KDF パラメータの V3 コンテナ、パスワード/パスワードなしモード
  - **利用可**: ネストパスとディレクトリ（`mkdir`、再帰 `add --recursive`、再帰削除）
  - **利用可**: 認証検証、CRC32 チェック、デフラグ、読み取り専用リカバリ
  - **利用可**: Header A / Header B / Footer の 3 ヘッダー副本
- フロントエンド
  - **利用可**: CLI の `create`、`verify`、`list`、`add`、`extract`、`remove`、`defrag`、`mkdir`、`rename`、`passwd`、`grow`、`mount`
  - **利用可**（`gui-mount`）: サイドバー + パンくずブラウザ（黒曜石/暗金テーマ）、ファイル/フォルダーインポート（ソース名を保持）、再帰ディレクトリ抽出と競合事前チェック、Ctrl/Shift 複数選択と一括削除/抽出、リネーム/外部で開く（単一選択のみ）、マウントパネル、フォルダーシンボリックリンク、最近のボルト、自動ロック、防截屏（Windows/macOS は winit 経由、Linux はベストエフォート）
- マウント
  - **Linux — 実装: はい; ビルド検証: はい; 実機検証: はい（読み書き）。** `fuser` アダプタ。クローズ時にファイル全体を書き戻し、コミット後にビューを更新する。
  - **macOS — 実装: はい（WebDAV ブリッジ）; ビルド検証: あり; 実機検証: プロトコルレベル（raw HTTP 統合テスト）。Finder での実機検証は今後。** 組み込み loopback WebDAV ブリッジ（`127.0.0.1` + ランダムセッショントークン）を Finder がネイティブにマウント。macFUSE などの外部ランタイムは不要。
  - **Windows — 実装: はい; ビルド検証: はい; 実機検証: はい（読み書き）。** `winfsp_wrs` + ユーザーインストールの WinFsp 2.1。書き込みはクローズ時にファイル全体を書き戻す。`winfsp_mount_tests` は作成、切り詰め上書き、mkdir/rename/delete をカバー（2026-10-02、このマシンで 4/4）
- その他
  - **利用可**: ファイル/フォルダーのシンボリックリンク（マウント中 + `--symlink` / GUI トグル）、CLI `add --recursive` は `-n` 省略時にソースフォルダー名をデフォルト使用、リカバリ `extract` は GUI と同じ競合/シンボリックリンク処理、GitHub release workflow（tag `v*`）、Apache-2.0 + [THIRD_PARTY.md](THIRD_PARTY.md)

## CLI

`sesame` バイナリには `cli` と `mount` feature が必要です:

```bash
cargo build --release --bin sesame --features "cli,mount"
```

```bash
sesame create treasure --size 500MB
sesame add treasure.sesame ./docs --recursive --no-password
sesame extract treasure.sesame project --output ./restored
sesame --lang ja list treasure.sesame
```

抽出は既定で**コピー**です。コンテナ内のファイルは残ります。`sesame extract --move` は検証済みの書き出し後にコンテナから削除します（隠された `--keep` は互換のための無操作です）。GUI の「移出」は別の確認です。`sesame passwd` はマスターキーの包み直しだけでファイル頁は書き換えません（新しいパスワードを空のまま確認するとパスワードなしになります）。`sesame grow --by 1GB` は容量をその場で拡張します（4 MiB ブロックへ切り上げ）。能力とリスクの正本は [STATUS.md](STATUS.md) です。

Windows でマウントする前に [WinFsp 2.x](https://github.com/winfsp/winfsp/releases)（Core + Developer）をインストールしてください。**macOS は組み込み loopback WebDAV ブリッジでマウントします**（GUI のマウントボタン、または CLI `sesame mount --webdav`）。Finder がネイティブ接続し、カーネル拡張は不要です。Linux はシステムの FUSE スタック（`/dev/fuse`）を使用し、デフォルトで**書き込み可能**です。

```powershell
cargo test --no-default-features --features mount --test winfsp_mount_tests -- --nocapture
```

Linux で `/dev/fuse` が利用可能な場合:

```bash
cargo test --no-default-features --features mount --test fuse_mount_tests -- --nocapture
```

## GUI とリカバリ

```bash
cargo build --release --bin sesame-gui
cargo run --release --bin sesame-gui --features gui-mount
```

ウィンドウを閉じるか最小化するとトレイに残り、マウントは維持されます。終了はトレイメニューから行います。

```bash
cargo build --release --bin sesame-recover --no-default-features --features recover
sesame-recover extract treasure.sesame --output ./recovered --skip-corrupted
```

## 開発

```bash
cargo test
cargo clippy --all-targets
```

詳細: [アーキテクチャ](docs/ja/ARCHITECTURE.md)、[GUI 設計](docs/ja/DESIGN.md)

## ライセンス

Apache-2.0。第三者依存は [THIRD_PARTY.md](THIRD_PARTY.md) を参照してください。変更を送る前に [CONTRIBUTING.md](CONTRIBUTING.md) を読み、セキュリティ問題は [SECURITY.md](SECURITY.md) の非公開手順で報告してください。
