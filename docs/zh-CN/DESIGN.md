# GUI 设计边界

**Languages:** [English](../en/DESIGN.md) | [简体中文](DESIGN.md) | [日本語](../ja/DESIGN.md)

## 信息架构

- **首次语言选择**（`views/locale_setup.rs`）：未持久化语言时，在锁定页之前显示三语选择卡片。
- **锁定页**（`views/locked.rs`）：居中卡片式打开/创建容器、最近列表、密码/无密码模式。
- **浏览页**（`views/browser.rs`）：左侧边栏（容器状态、添加文件/文件夹、挂载控制、锁定/设置）+ 主区面包屑导航与文件列表。
- **设置页**（`views/settings.rs`）：分组展示自动锁定、防截屏开关、WinFsp 链接、主题切换、**语言选择**。

视觉主题：深色「黑曜石与暗金」、浅色「珍珠白与石板灰」（`theme.rs`）；自绘 SVG 图标组（`assets/icons/`）；加大间距与圆角。

## 已实现交互

- 面包屑路径浏览（`browser.rs` + `current_dir`）
- 文件/文件夹导入；文件夹导入在容器内保留源目录名
- 单文件与递归目录提取，写入前冲突预检（不静默覆盖普通文件/目录）
- **多选**：Ctrl 切换、Shift 范围、右键多选惯例（`selected_paths`、`selection_anchor`）
- **删除确认**（`dialog.rs`）：单条与批量删除均需确认；批量最多展示五条路径
- **提取说明**（`dialog.rs`）：首次提取可选说明对话框，支持「不再显示」
- **符号链接说明**（`dialog.rs`）：说明文件夹仅根路径变 symlink；支持「不再显示」
- **最近容器解锁**（`dialog.rs`）：从最近列表打开时弹出密码输入
- **批量提取**：选一个目标目录，统一预检（all-or-nothing），状态栏显示提取数量
- **仅单选**：重命名、外部打开
- 解密到临时文件后 `open_external`
- 挂载/卸载（`gui-mount`）与 symlink 开关（需已挂载）；支持文件与文件夹符号链接
- 最近容器（`recent.rs`，配置目录 JSON）
- 自动锁定（默认 10 分钟，可配置）
- 防截屏（`anti_capture.rs`，Settings 开关，解锁后生效）：`ViewportCommand::ContentProtected` → winit `set_content_protected`（Windows/macOS）；Linux 同路径调用（当前为 no-op），说明随 X11/Wayland/KDE 环境变化
- CJK 字体：内嵌 Noto Sans SC 子集（`assets/fonts/`）

## 提取冲突交互

- `ExtractConflict` 在状态栏提示；用户需手动重新选择输出目录（不自动重开文件夹选择器）。
- `ExtractMountReadOnly` 拒绝向当前挂载目录提取，输出应写入挂载视图之外的目录。
- 目标路径上的符号链接可在写入前静默清除。

## 边界

- 防截屏不是安全边界；Linux（X11/Wayland）无通用应用内 API，KDE 用户可配窗口规则作为补充。
- macOS 防截屏为尽力实现；macOS 15+ ScreenCaptureKit 可能仍能捕获。
- Linux FUSE 与 Windows WinFsp 挂载可写（关闭时整文件写回）。macOS 走 WebDAV 桥，Finder 行为尚未在 Mac 上复验。
- 批量提取不与已有目录合并；顶层同名冲突则整次操作拒绝。

## 构建

```bash
cargo run --features gui-mount --bin sesame-gui
```
