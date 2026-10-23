# Contributing translations

**Languages:** [English](CONTRIBUTING-i18n.md) | [简体中文](../zh-CN/CONTRIBUTING-i18n.md) | [日本語](../ja/CONTRIBUTING-i18n.md)

ClosedSesame uses two translation surfaces:

## 1. Runtime strings (`locales/`)

Managed by [rust-i18n](https://github.com/longbridge/rust-i18n).

| File | Locale |
|------|--------|
| `locales/en.yml` | English (baseline — must be complete) |
| `locales/zh-CN.yml` | Simplified Chinese |
| `locales/ja.yml` | Japanese |

### Rules

1. Add new user-visible strings with `ts!("dotted.key")` (library/GUI) or `ts!` in binaries.
2. Add the key to **all three** YAML files in the same change.
3. Use `%{name}` placeholders in YAML and `name = value` in `ts!` calls.
4. Store validation error keys in `i18n::keys` when reused across modules.
5. Run `python3 scripts/check_locales.py` before pushing (`python` on Windows).

### Locale resolution

1. `--lang` (CLI global flag)
2. `CLOSED_SESAME_LANG`
3. System locale (`sys-locale`, `zh` → `zh-CN`)
4. Fallback: `en`

GUI language is persisted in `closed-sesame/settings.json` under the OS config directory.

## 2. Documentation (`docs/{en,zh-CN,ja}/`, `README*.md`)

Markdown is maintained per language. Each doc starts with language links. When changing behavior, update every language copy or add a follow-up task.

## Fonts

GUI text uses **Source Sans 3** for Latin and **Noto Sans CJK** (SC and JP Regular) for Chinese and Japanese, all SIL OFL 1.1, under `assets/fonts/`. Document license changes in `THIRD_PARTY.md`.
