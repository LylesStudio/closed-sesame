# 译文贡献指南

**Languages:** [English](../en/CONTRIBUTING-i18n.md) | [简体中文](CONTRIBUTING-i18n.md) | [日本語](../ja/CONTRIBUTING-i18n.md)

ClosedSesame 有两套译文体系：

## 1. 运行时字符串（`locales/`）

使用 [rust-i18n](https://github.com/longbridge/rust-i18n) 管理。

| 文件 | 语言 |
|------|------|
| `locales/en.yml` | 英语（基准，必须完整） |
| `locales/zh-CN.yml` | 简体中文 |
| `locales/ja.yml` | 日语 |

### 约定

1. 新增用户可见字符串使用 `ts!("键名")`。
2. 同一变更中补齐三种语言的 YAML key。
3. YAML 占位符用 `%{name}`，代码中用 `name = value` 传参。
4. 跨模块复用的校验错误键放在 `i18n::keys`。
5. 提交前运行 `python3 scripts/check_locales.py`（Windows 使用 `python`）。

### 语言解析顺序

1. `--lang`（CLI 全局参数）
2. `CLOSED_SESAME_LANG`
3. 系统 locale（`zh` 映射为 `zh-CN`）
4. 回退 `en`

GUI 语言保存在系统配置目录下的 `closed-sesame/settings.json`。

## 2. 文档（`docs/{en,zh-CN,ja}/`、`README*.md`）

Markdown 按语言分别维护；每篇文档顶部有语言切换链接。行为变更时请同步更新各语言版本。

## 字体

GUI 文字使用 **Source Sans 3**（英文）和 **思源黑体 / Noto Sans CJK**（简体与日文 Regular，SIL OFL 1.1），文件在 `assets/fonts/`。许可变更请更新 `THIRD_PARTY.md`。
