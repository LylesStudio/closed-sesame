# ClosedSesame 🔒

**Languages:** [English](README.md) | [简体中文](README.zh-CN.md) | [日本語](README.ja.md)

ClosedSesame 是一个使用 Rust 编写的加密文件容器实验项目。文件存放在 `.sesame` 容器中，当前格式使用 AES-256-GCM、Argon2id/HKDF 和页级认证加密。

项目仍处于早期阶段；请勿把唯一副本只保存在容器中。

## 宣传片

`v0.1.0` release 附带已经完成的宣传片：

- [English promo video（MP4）](https://github.com/Silver-Aurora/closed-sesame/releases/download/v0.1.0/closed-sesame-promo-en.mp4)
- [中文宣传片（MP4）](https://github.com/Silver-Aurora/closed-sesame/releases/download/v0.1.0/closed-sesame-promo-zh.mp4)

## 国际化

运行时界面与 CLI 支持 **英语**、**简体中文**、**日语**。

- CLI / 恢复工具：`--lang en|zh-CN|ja` 或环境变量 `CLOSED_SESAME_LANG`
- GUI：首次启动语言选择（未设置时）；设置 → 语言（保存在系统配置目录）
- 译文目录：[`locales/`](locales/)；维护说明见 [docs/zh-CN/CONTRIBUTING-i18n.md](docs/zh-CN/CONTRIBUTING-i18n.md)

## 当前能力

状态说明：**实现**表示代码路径存在；**编译验证**表示对应平台已执行构建/单元测试；**实机验证**表示真的挂载并通过文件访问测试。三者不可互相替代；**Roadmap** 表示尚未完成。

- 容器
  - **可用**：创建自描述 KDF 参数的 V3 容器；密码和无密码模式。
  - **可用**：嵌套路径与目录（`mkdir`、递归 `add --recursive`、递归删除）。
  - **可用**：认证校验、CRC32 文件校验、碎片整理和只读恢复流程。
  - **可用**：Header A、Header B、Footer 三份头部副本。
- 前端
  - **可用**：CLI 的 `create`、`verify`、`list`、`add`、`extract`、`remove`、`defrag`、`mkdir`、`rename`、`passwd`、`grow`、`mount`。
  - **可用**（`gui-mount`）：侧边栏 + 面包屑浏览（黑曜石/暗金主题）、文件/文件夹导入（保留源文件夹名）、递归目录提取与冲突预检（不静默覆盖普通文件/目录；符号链接可清除后写入）、Ctrl/Shift 多选与批量删除/提取、重命名/外部打开（仅单选）、挂载面板、文件夹符号链接、最近容器、自动锁定、防截屏（Windows/macOS 经 winit；Linux 尽力而为并随环境说明）。
- 挂载
  - **Linux — 实现：是；编译验证：是；实机验证：是（读写）。** `fuser` 适配器，关闭时整文件写回，提交后刷新视图。
  - **macOS — 实现：是（WebDAV 桥）；编译验证：是；实机验证：协议级（raw HTTP 集成测试）；Finder 实机待复验。** 内置 loopback WebDAV 桥（`127.0.0.1` + 随机会话 token），Finder 原生挂载，无需 macFUSE 等任何外部运行时。
  - **Windows — 实现：是；编译验证：是；实机验证：是（读写）。** `winfsp_wrs` + 用户安装的 WinFsp 2.1。写入在句柄关闭时整文件写回；`winfsp_mount_tests` 覆盖新建、截断覆盖、mkdir/rename/delete（2026-10-02 本机 4/4）。
- 其他
  - **可用**：文件与文件夹符号链接生命周期（挂载态 + `--symlink` / GUI 开关）；CLI `add --recursive` 未指定 `-n` 时默认使用源文件夹名；恢复工具 `extract` 与 GUI 提取的冲突/符号链接处理一致；GitHub release workflow（tag `v*`）；Apache-2.0 + [THIRD_PARTY.md](THIRD_PARTY.md)。

## CLI

`sesame` 二进制需要 `cli` 和 `mount` feature：

```bash
cargo build --release --bin sesame --features "cli,mount"
```

常用命令与当前 CLI 定义一致：

```bash
# 创建；默认会提示输入密码
sesame create treasure --size 500MB

# 明确创建无密码容器
sesame create treasure --size 500MB --no-password

# 添加、列出、提取和删除
sesame add treasure.sesame ./docs --recursive --no-password
sesame mkdir treasure.sesame projects/new --no-password
sesame rename treasure.sesame old.txt new.txt --no-password
sesame list treasure.sesame
sesame extract treasure.sesame secret.txt --output ./restored
sesame extract treasure.sesame secret.txt --output ./restored --move
sesame passwd treasure.sesame
sesame grow treasure.sesame --by 1GB
sesame remove treasure.sesame secret.txt

# 校验和碎片整理
sesame verify treasure.sesame
sesame defrag treasure.sesame
sesame defrag treasure.sesame --force

# 可写挂载；Linux/macOS 使用目录，Windows 可使用盘符（如 X:）
sesame mount treasure.sesame /mnt/treasure
```

CLI 和恢复工具从终端读取密码时不回显，并在操作结束后尽快清理密码字符串。读取无密码容器时，`verify`、`list`、`mount`、`add`、`extract` 和 `remove` 接受 `--no-password`。挂载成功后 CLI 会等待用户按 Enter，再显式卸载；`defrag` 当前总会提示密码且没有无密码参数。

Windows 运行挂载前需从 [WinFsp 官方发布页](https://github.com/winfsp/winfsp/releases) 另行安装 WinFsp 2.x（Core + Developer）。**macOS 通过内置 loopback WebDAV 桥挂载**（GUI 挂载按钮，或 CLI `sesame mount --webdav`）；Finder 原生连接，无需内核扩展。Linux 需系统 FUSE 栈（`/dev/fuse`），默认**可写**挂载。

读出默认为**复制**：文件留在容器里。`sesame extract --move` 在校验写出后才从容器删除（隐藏的 `--keep` 无效果，只为兼容旧命令）。GUI「移出」是单独确认；「打开」解密到会话缓存并监听修改，关闭后自动写回容器，冲突时提供时间戳自动更名保护。`sesame passwd` 只重包主密钥、不重写文件页（新密码留空并确认则改为无密码）；`sesame grow --by 1GB` 就地扩容（向上取整到 4 MiB 块）。能力状态与风险以 [STATUS.md](STATUS.md) 为准。

在 Linux 上，当 `/dev/fuse` 可用时可运行真实挂载测试：

```bash
cargo test --no-default-features --features mount --test fuse_mount_tests -- --nocapture
```

安装 WinFsp 后，可在 Windows 上运行真实挂载测试：

```powershell
cargo test --no-default-features --features mount --test winfsp_mount_tests -- --nocapture
```

完整参数以 `sesame --help` 和 `sesame <command> --help` 为准。

## GUI 与恢复工具

默认 feature 会构建 GUI：

```bash
cargo build --release --bin sesame-gui
cargo run --release --bin sesame-gui

# Windows/macOS/Linux 挂载面板（需对应平台运行时）
cargo run --release --bin sesame-gui --features gui-mount
```

关闭或最小化窗口会把程序留在系统托盘，挂载继续可用；托盘菜单的「退出」才会真正结束。GUI 采用侧边栏 + 主内容区布局（`views/locked`、`browser`、`settings`），支持树形路径浏览、文件/文件夹导入（根目录导入保留文件夹名）、单文件与递归目录提取（目标冲突时拒绝写入）、Ctrl/Shift 多选批量删除与提取、重命名、mkdir、外部打开、自动锁定与挂载（`gui-mount`）。批量提取采用 all-or-nothing 预检；重命名与外部打开仅支持单选。提取不可写入只读挂载目录（`ExtractMountReadOnly`）。防截屏在解锁后生效：Windows/macOS 经 winit `set_content_protected`；Linux 无通用 API，设置页说明随 X11/Wayland/KDE 环境变化。

Linux GUI 当前使用 eframe 的 X11/Glow 窗口后端，原生文件对话框使用 `xdg-portal`；项目没有启用 eframe 的 Wayland 窗口后端。

恢复工具使用独立 feature：

```bash
cargo build --release --bin sesame-recover --no-default-features --features recover
sesame-recover verify treasure.sesame
sesame-recover list treasure.sesame
sesame-recover extract treasure.sesame --output ./recovered --skip-corrupted
```

恢复流程只读打开原容器，并把可读取内容写入输出目录。

## 开发

工具链固定在 `rust-toolchain.toml`：

```bash
cargo test
cargo clippy --all-targets

# Windows 挂载 feature 编译检查
cargo check --no-default-features --features cli,mount --bin sesame
```

Windows 上启用 `mount` 的构建已通过检查，但仍需在安装 WinFsp 的机器上执行实际挂载验证。macOS mount（WebDAV 桥）由 self-hosted macOS runner 编译验证；Finder 实机挂载行为待复验（见 [STATUS.md](STATUS.md)）。

更多细节见 [架构说明](docs/zh-CN/ARCHITECTURE.md) 与 [GUI 设计边界](docs/zh-CN/DESIGN.md)。

## 格式兼容性

当前只写入和读取 V3。V3 在 256B 头部中以大端固定字段保存 KDF algorithm/schema、`memory_kib`、`iterations`、`parallelism` 和 `output_len`。密码容器默认使用 Argon2id 64 MiB、3 次迭代、并行度 1、32B 输出；已有容器始终按自己的头部参数打开。

这是破坏兼容的变更：V2 会被明确拒绝并提示重新创建，不提供原地迁移器。请先用旧版本提取 V2 文件，再创建 V3 容器。

`--no-password` 使用独立、公开且可重复的 HKDF 派生，不抵抗人工攻击、离线分析或恶意软件；它不是弱密码的替代品。密码模式的默认 KDF 在本开发机（Windows x86_64，Intel Core i7-14700K，release 构建）单次简单基准为 78.6905 ms。

## License

Apache-2.0。第三方运行时和挂载依赖见 [THIRD_PARTY.md](THIRD_PARTY.md)。提交改动前请阅读 [CONTRIBUTING.md](CONTRIBUTING.md)；安全问题请按 [SECURITY.md](SECURITY.md) 私下报告。
