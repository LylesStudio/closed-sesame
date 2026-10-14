# 翻訳への貢献

**Languages:** [English](../en/CONTRIBUTING-i18n.md) | [简体中文](../zh-CN/CONTRIBUTING-i18n.md) | [日本語](CONTRIBUTING-i18n.md)

## 1. 実行時文字列（`locales/`）

| ファイル | 言語 |
|----------|------|
| `locales/en.yml` | 英語（基準） |
| `locales/zh-CN.yml` | 簡体字中国語 |
| `locales/ja.yml` | 日本語 |

新しい UI 文字列は `ts!("key")` を使い、3 ファイルすべてにキーを追加してください。`scripts/check_locales.ps1` で整合性を確認します。

## 2. ドキュメント

`docs/{en,zh-CN,ja}/` と `README*.md` を言語ごとに更新します。

## フォント

`assets/fonts/NotoSansSC-Regular.otf`（SIL OFL 1.1）を GUI に同梱しています。
