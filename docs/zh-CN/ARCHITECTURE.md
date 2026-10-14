# ClosedSesame 架构

**Languages:** [English](../en/ARCHITECTURE.md) | [简体中文](ARCHITECTURE.md) | [日本語](../ja/ARCHITECTURE.md)

本文描述当前代码。核心原则是让容器格式和业务用例不依赖具体前端或挂载后端。

## 模块边界

```text
CLI (`src/cli`) ───────┐
GUI (`src/gui`) ───────┼──> app ──> container ──> crypto / keymgmt
恢复工具 (`src/recover`)┘       └──> error / i18n

mount (`src/mount`) ──> SharedVault ──> FUSE（读写）/ WebDAV 桥 / winfsp_wrs（读写）
```

- `app`：用例层（凭据、vault CRUD、恢复、符号链接注册表 `symlink.rs`）。
- `app/vault.rs`：共享提取/添加/删除路径；`ensure_extract_destination` 在写入前预检目标冲突。
- `app/symlink.rs`：会话注册表，挂载期间在 `original_path` 维护文件与**目录**符号链接。
- `container/path.rs`：容器内路径规范化（`/` 分隔、禁止 `..`）。
- `mount/vfs.rs`：从扁平 `FileEntry` 列表构建 inode 树，支持任意深度 lookup/readdir。
- `mount_hints.rs`：WinFsp 官方下载链接与错误补充说明（CLI/GUI 共用）。
- `gui/views/`：锁定页、浏览页、设置页、首次语言选择 `locale_setup`；`gui/dialog.rs` 承载删除/提取/符号链接/最近解锁对话框；`gui/selection.rs` 对批量删除做父子路径去重。
- `gui/anti_capture.rs`：经 `ViewportCommand::ContentProtected` 调用 winit；Linux 按环境返回能力说明。
- `i18n`：基于 `rust-i18n` 的集中式译文（`locales/*.yml`）。
- Windows 挂载通过 **winfsp_wrs**（MIT crate）调用用户安装的 WinFsp 运行时（GPLv3 + FLOSS 豁免）。

## 目录模型

- `FileEntry.path` 使用 `/` 分隔的相对路径；`is_directory=true` 表示空目录条目。
- 添加文件时自动创建缺失的中间目录；`mkdir`、`rename_path`、递归 `remove_path` 在 `ContainerWriter` 中实现。
- 文件夹导入（GUI 与 CLI `--recursive` 且未指定 `-n`）在容器内保留源文件夹名作为顶层目录。
- 打开容器时 `validate_entry_tree` 检查无孤儿路径、无重复、目录不占用数据页。

## 提取与冲突策略

- `ensure_extract_destination`：目标不存在或仅为符号链接时允许；普通文件/目录返回 `ExtractConflict`。
- 向当前挂载目录提取时返回 `ExtractMountReadOnly`，避免把输出经由挂载视图写回容器。
- 目录提取收集所有子文件，对全部目标路径预检（批量提取为 all-or-nothing），再经 `clear_symlink_for_write` 写入。
- `recovery::extract_all` 复用相同 helper，与 GUI/CLI 提取行为一致。

## 挂载

- Linux：共享 `fuser` 适配器（可写，copy-on-write-on-close）。macOS：内置 loopback WebDAV 桥（Finder 原生挂载，无需外部运行时）。macFUSE/FSKit 适配器已移除；macOS 上的 `mount_vault` 会失败，并提示改用 `sesame mount --webdav`。
- Windows：`winfsp_wrs::FileSystemInterface`，路径逐段 lookup；`read_directory` 支持子目录枚举。
- 挂载前 `mount::check_prerequisites()`；缺失 WinFsp 时错误信息含 https://github.com/winfsp/winfsp/releases

## 符号链接

- 仅在 vault 已挂载且用户启用时生效（`--symlink` / GUI 开关）。
- **文件**：`move_with_symlink` 将源文件替换为指向 `{mount_point}/{container_path}` 的 symlink。
- **文件夹**：`move_dir_with_symlink` 删除源目录树（内容已在容器内），在原路径创建目录 symlink。
- 锁定/卸载时清理注册表；重新挂载后 `restore_symlinks` 恢复链接。

## 前端

- CLI：`create`、`verify`、`list`、`mount`、`defrag`、`add`（`--recursive`、`--symlink --mount-point`）、`extract`、`remove`、`mkdir`、`rename`。
- GUI（`--features gui-mount`）：侧边栏布局、面包屑浏览、Ctrl/Shift 多选、带确认对话框的批量删除/提取、单选重命名/外部打开、挂载面板、文件夹符号链接、最近容器、自动锁定、防截屏（Windows/macOS 经 winit；Linux 尽力而为）、首次语言选择、语言设置。
- `sesame-recover`：只读 verify/list/extract，保留嵌套目录结构，并遵循与 GUI 相同的提取冲突规则。

## 许可

项目以 **Apache-2.0** 分发；第三方运行时说明见 [`THIRD_PARTY.md`](THIRD_PARTY.md) 与根目录 [`THIRD_PARTY.md`](../THIRD_PARTY.md)。

## 验证状态

- **编译验证**：Windows/Linux CI 覆盖 `cli,mount,gui-mount,recover`；不安装 WinFsp 也可编译。
- **实机挂载**：Linux 在 CI 提供 `/dev/fuse` 时运行 `fuse_mount_tests`。Windows 端到端挂载与目录符号链接测试（`winfsp_mount_tests`、`winfsp_symlink_tests`）需本机安装 WinFsp。macOS 通过回环 WebDAV 桥挂载（`webdav_tests` 覆盖协议）。macFUSE/FSKit 路径已移除。Finder 在真机上的行为尚未复验。
