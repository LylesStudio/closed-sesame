# ClosedSesame — 项目状态权威记录

> 本文件是项目当前状态的**权威记录**（authoritative source of truth）。
> 每次完成功能迭代、平台验证或发现风险时更新本文件。README/docs 中的能力描述以本文件为准。
> 最近更新：2026-10-14（开源发布加固：格式/Clippy/CI、隐私边界、跨平台文档与 GitHub 发布 workflow）
> 上次功能更新：2026-10-02（GUI 交互重构，见 §9）；2026-08-02（WinFsp 可写挂载）

---

## 1. 本轮迭代落地总览

| # | 需求 | 状态 | 说明 |
|---|------|------|------|
| 1 | 非挂载模式打开容器文件：可写、关闭后写回并清除缓存 | ✅ 完成 | 会话级缓存目录 + notify 监听 + 1.5s 静默防抖写回 + 锁定/退出强制 flush 与安全擦除 |
| 2 | 挂载虚拟盘可写 | ✅ 完成 | Linux FUSE 与 Windows WinFsp 均为 copy-on-write-on-close；macOS WebDAV 协议级可写，Finder 未复验 |
| 3 | 读出后默认清除容器内文件，可配置保留 | ✅ 已改语义 | 2026-10-02 起读出默认**保留**；移出是单独确认。见 §4.2、§10 |
| 4 | macOS 无法挂载 | ✅ 方案落地 | loopback WebDAV 桥（零 macFUSE 依赖），可写闭环；协议级测试通过，Finder 真机待验证 |
| 5 | 平台验证与风险排查 | ✅ Linux 完成 | 当前 canonical 门禁：171 passed / 0 failed / 2 ignored；风险清单见 §5 |
| 6 | GUI 美化 + 双主题 | ✅ 完成 | 深色 黑+金 / 浅色 灰白+银，四视图截图验证（浅色配色于 2026-10-02 改为羊皮纸+黄铜，见 §9） |
| 7 | GUI 交互体验重构与缺陷修复 | ✅ 完成 | 2026-10-02：主题淡变、搜索/排序/行内重命名/键盘导航、容量与挂载状态、设置与对话框动效；见 §9 |

## 2. 平台挂载矩阵

| 平台 | 后端 | 读写 | 构建验证 | 真机验证 |
|------|------|------|----------|----------|
| Linux | FUSE (`fuser`) | **读写** | ✅ CI | ✅ 本机 7/7（读、写新建、覆盖截断、mkdir/rename/unlink/rmdir） |
| Windows | WinFsp (`winfsp_wrs`) | **读写** | ✅ CI | ✅ 2026-10-02 本机 `winfsp_mount_tests` 4/4（读、新建、截断覆盖、mkdir/rename/delete）。关闭句柄时刷缓存并提交 |
| macOS | **loopback WebDAV 桥** | **读写** | ✅ | ⚠️ 协议级集成测试通过（raw HTTP）；Finder 真机挂载未验证（无 Mac 设备） |
| macOS | ~~旧 macFUSE FSKit 路径~~ | **已移除** | — | 代码与声明均已删除；macOS 挂载仅由 WebDAV 桥提供 |

macOS 默认走 WebDAV：GUI 挂载按钮 / CLI `sesame mount --webdav` 启动回环服务，macOS 上自动 `osascript mount volume` 调起 Finder。服务仅绑 `127.0.0.1` 随机端口 + 每会话随机 URL token，LOCK/UNLOCK 仅作协议应答（单用户回环语义）。

## 3. 数据完整性机制（防损坏/防丢失，第一优先级）

1. **两阶段提交**（`ContainerWriter`）：替换/新增文件时先写新页→提交位图+文件表→再释放旧页。崩溃最坏结果是页泄漏（defrag 可回收），永不损坏旧版本。
2. **删除安全序**：先提交删除后的文件表，再释放页。杜绝文件表指向已释放页的悬空引用。
3. **全链路 fsync**：`persist_bitmap` / `persist_table` / `write_new_pages` / `create` 均 `sync_data`/`sync_all`。
4. **挂载写路径 COW-on-close**：FUSE 写入先落内存缓冲，`flush`/`release` 时经两阶段提交落盘；`release` 兜底提交。
5. **跨入口串行化**：`SharedVault::mutation_lock`（单一 writer 互斥锁）串行 FUSE flush、WebDAV 请求、GUI 操作、缓存写回；每次提交后 VFS 快照重载。
6. **缓存冲突保护**：写回前对比容器条目 `modified_at` 与提取锚点；冲突弹窗三选（覆盖容器 / 保留两者-时间戳自动更名 / 放弃修改）；锁定时冲突一律自动保留两者，不丢编辑。

## 4. 功能细节

### 4.1 缓存打开闭环（GUI「打开」）
- 解锁即建 `$TMP/closed-sesame/<uuid>/`（Unix 0700）；缓存文件名 `<UTC时间戳>-<原名>` 避免同名冲突。
- `notify` 监听修改；1.5s 静默视为"关闭"（跨平台无法真检测 close，xdg-open 等立即 detach）。
- 写回走 `replace_file_data`（保留 created_at/符号链接元数据）并刷新列表与挂载视图。
- 锁定/退出：全部脏文件 flush（冲突→保留两者）→ 逐文件清零后删除；写回失败的文件保留在缓存目录并提示。

### 4.2 读出为复制，移出单独确认
- GUI「读出」只解密复制到所选文件夹，容器内原文件保留。
- 「移出」单独确认：写出并校验后再从容器删除。批量部分失败只删除已成功写出的条目。
- `settings.json` 仍接受旧字段 `keep_files_on_extract`（缺省为 true），界面不再提供开关，读出也不再读取它。
- CLI：`sesame extract` 默认保留；`--move` 才删除。隐藏的 `--keep` 无效果，仅为兼容旧命令。`打开`（缓存流）不受影响。

### 4.3 FUSE 可写挂载
- 实现 `create/open/write/setattr(truncate)/mkdir/unlink/rmdir/rename/flush/fsync/release`；权限 0o644/0o755。
- O_TRUNC 经 `setattr(size)` 按路径截断所有打开句柄（内核不一定带 fh）——真机测试发现并修复。
- `refresh_vault` FUSE 后端已接线（修复了 GUI 增删后挂载视图过期、inode→页映射错位的重大风险）。
- `path_is_within_mount` 在 Linux 改为大小写敏感比较（修复误判）；GUI Linux 默认挂载点 `$TMP/closed-sesame-mount`。

### 4.4 WebDAV 桥
- `src/mount/webdav.rs`：OPTIONS/PROPFIND(depth 0,1)/GET/HEAD/PUT/MKCOL/DELETE/MOVE/LOCK/UNLOCK。
- PUT 存在→`replace_file_data`，不存在→`add_file_data`；MOVE 映射 rename（Overwrite: F → 412）。
- 仅 loopback + token 鉴权；无 TLS（回环可接受，见 §5 风险）。

### 4.5 GUI
- 主题令牌：Cave（黑曜 + 金 #D4AF37）、Parchment（羊皮纸 #F3F1EC + 旧黄铜 #966C18，主按钮深褐 #2E2922）。`Theme` 为持久化选择，`Palette` 为每帧实际颜色，切换时约 350ms 整体淡变。
- 锁定页：呼吸光晕、旋转刻度环、错误抖动、Enter 提交、最近列表相对时间、密码强度条与容量预设。
- 浏览器：面包屑、文件夹内搜索、列头排序（名称/大小/修改时间，目录优先、自然序）、行内重命名、键盘导航、拖放覆盖层、侧栏容量条与挂载状态、通知面板。
- 设置页：主题迷你预览、语言分段切换、自动锁定开关与滑块。对话框遮罩淡入、卡片滑入。
- 2026-07-31 一并修复：`DesignCanvas::inner_rect` 未加 origin（非默认窗口尺寸整体左偏）；eframe `wayland` feature。
- 截图：debug 构建 `CLOSED_SESAME_DEMO`（仅 debug_assertions）可组合 `create`/`error`/`settings`/`select`/`delete`/`search`/`rename`/`mkdir`/`bell`。

## 5. 风险清单（已知限制/待办）

| 级别 | 风险 | 状态/缓解 |
|------|------|-----------|
| 高 | macOS Finder WebDAV 真机行为未验证（可能需兼容 Finder 怪癖：AppleDouble `._*`、chunked PUT、LOCK 语义） | 协议测试已过；需 Mac 设备复验 |
| 中 | FUSE / WinFsp 写路径为整文件 COW，大文件编辑写放大（整文件重加密） | 短期可接受；页级差量写仍在路线图，本轮不做 |
| 中 | 元数据块每次整 4MB 加密（debug ~0.5s/次，release ~10ms）；大量小文件操作有可见延迟 | 后续可批量提交/差量加密 |
| 低 | 跨进程写同一容器 | 边车锁 `{容器路径}.lock` + 进程内引用计数；另一进程得到 `ContainerBusy`。见 §10 |
| 低 | FUSE VFS 重载后 inode 可能偏移；1s 内核 TTL 缓解 | 标准做法（generation=0），未观察到实际问题 |
| 低 | WebDAV 无 TLS（仅 127.0.0.1 + 随机 token） | 同机恶意进程可嗅探回环流量属极端场景 |
| 低 | 缓存"关闭检测"为 1.5s 静默近似；编辑器持续写入期间会多次写回 | 语义安全（幂等两阶段提交） |
| 低 | anti-capture 在 Linux 仍为 best-effort | 设置页已注明能力边界 |
| 低 | FUSE `forget` 未实现（内核 inode 缓存只增） | 短挂载场景影响可忽略 |

## 6. 路线图

- ~~WinFsp 写路径对齐~~（2026-08-02 回调落地；2026-10-02 本机写入读回复验，见 §10）
- ~~跨进程容器文件锁~~（2026-10-02：边车锁，见 §10）
- ~~GUI 大批量操作后台线程化 + 进度展示~~（2026-10-02，见 §10）
- 页级差量写（替代整文件 COW，降低大文件写放大）
- 元数据块差量/批量提交（替代整 4MB 块每次重写）
- macOS Finder 挂载复验（本轮不处理）

## 7. 验证记录（2026-07-31，Linux 本机）

- `cargo test --locked --all-features`：**178 passed / 0 failed / 2 ignored**（2026-10-14；Windows 集成测试由 Windows runner 执行）。
- 挂载启动优化：`validate_entry_tree` 由 O(n²) 降为 O(n·depth)；空符号链接注册表时挂载不再重复读取文件表。
- 缓存写回：重开先 flush 再重建；已挂载且挂载点为本机路径时「打开」直接走挂载路径；冲突锚点改为从容器实时读取（可正确识别挂载侧编辑并生成时间戳冲突副本）。
- FUSE 真机：`fuse_mount_tests` 7/7（含写新建、覆盖截断、mkdir/rename/unlink/rmdir、错误密码、空容器）。
- WebDAV：`webdav_tests` 2/2（OPTIONS/PROPFIND/GET/PUT 新建+覆盖/MKCOL/MOVE/DELETE/LOCK/UNLOCK/错误 token 404/百分号编码路径），并回读容器验证最终状态。
- CLI：`cli_tests` 8/8（2026-10-02 起断言改为：默认 extract 保留，`--move` 才删除）。
- GUI：四视图人工截图核验（dark/light × locked/browser）。
- `cargo clippy --all-targets --locked --no-default-features --features cli,mount,recover -- -D warnings`：通过。

## 8. 本轮主要变更文件

- 容器：`src/container/writer.rs`（两阶段提交 + `replace_file_data` + fsync）、`src/container/mod.rs`
- 挂载：`src/mount/shared.rs`(新)、`fuse_state.rs`(新)、`fuse_adapter.rs`(重写可写)、`webdav.rs`(新)、`session.rs`、`mod.rs`、`linux.rs`、`macos.rs`、`deps.rs`、`vfs.rs`、`mount_hints.rs`
- GUI：`src/gui/cache.rs`(新)、`app.rs`、`actions.rs`、`dialog.rs`、`theme.rs`、`layout.rs`、`widgets.rs`、`views/*`、`settings.rs`
- CLI：`src/cli/main.rs`（`extract --keep`、`mount --webdav`）
- 测试：`tests/cli_tests.rs`、`fuse_mount_tests.rs`、`webdav_tests.rs`(新)
- i18n：`locales/{en,zh-CN,ja}.yml`

## 9. GUI 交互重构（2026-10-02）

在 §4.5 视觉语言之上重做交互，并修掉重构中暴露的缺陷。

### 9.1 交互

- 主题与色板分离：用户选择持久化，每帧颜色可线性混合，切换主题整体淡变。
- 锁定页：呼吸光晕、旋转刻度环、密码错误卡片抖动、Enter 提交、最近容器相对时间、创建表单密码强度与容量预设。
- 浏览器：面包屑、当前目录搜索、列头排序、修改时间、行内重命名（默认选中主干名）、键盘导航、拖放覆盖层、空目录虚线落点；侧栏金库卡片（容量条）与挂载卡片，矮窗口时侧栏滚动、Settings/Lock 钉在底部。
- 设置页主题色卡与自动锁定滑块；对话框遮罩淡入与卡片位移动画。
- 浅色由灰白+银改为羊皮纸+旧黄铜，与深色共用金属色。

### 9.2 修复

| 问题 | 处理 |
|------|------|
| 主题、自动锁定从不写入 `settings.json` | `GuiSettings` 增加字段并在切换时保存 |
| 右键「重命名」直接以原名提交 | 改为行内编辑；Enter/点击空白提交，Esc 取消 |
| Shift 范围选择按名称序而非可见序 | 按当前过滤+排序后的行序 |
| 删除当前目录后面包屑指向失效路径 | 回退到最近仍存在的父目录 |
| 空文件夹名可提交；创建容器时空密码要到保存框之后才报错 | 提交前校验 |
| 自动锁定只在点击时重置，阅读/滚动中会被锁定 | 指针、滚轮、按键都重置空闲计时 |
| Windows 上打开文件闪黑色控制台 | `CREATE_NO_WINDOW` |
| 近黑背景光晕出现抖动噪点；浅色 `accent_glow` 预乘 alpha 非法 | 关闭 egui 渐变抖动，光晕环 6→16，修正预乘色 |
| `/dev/fuse` 错误在 Windows 上附带 WinFsp 下载链接 | 按失败的运行时选 URL；FUSE 提示不含下载链接 |

### 9.3 验证（2026-10-02，Windows 本机）

- `cargo build --features gui-mount --bin sesame-gui` 通过。
- `cargo test --all-features --lib`：74 passed。`i18n_tests` 3 passed。`scripts/check_locales.ps1`：245 键 × en/zh-CN/ja 一致。
- 截图迭代：dark/light × locked/browser，以及 settings、删除确认、搜索+新建文件夹、行内重命名、多选。
- 当时把 `winfsp_mount_tests` 的 3 个写入失败记成「本机环境、与本次无关」。复验后确认是写路径未在句柄关闭时落盘，已在 §10 修复。

## 10. 容器维护与 WinFsp 写入（2026-10-02）

### 10.1 容器维护

- **改密码**：重包主密钥并重写三份头部，不重写文件页，容器 ID 不变。两栏都留空（CLI 则把新密码留空并确认）改为无密码。GUI 与 `sesame passwd` 在已挂载时拒绝。
- **就地扩容**：按明文增量向上取整到 4 MiB 数据块，扩展位图并重写 Footer。`sesame grow --by SIZE`。不新建容器再迁入。
- **GUI 设置页**（已解锁）：校验、碎片整理、恢复到文件夹、改密码、扩容。碎片整理与改密、扩容要求先卸载。碎片整理前会暂时放开本进程的容器锁，完成后再收回。
- **读出 / 移出**：读出只复制。移出单独确认，写出后再删除。CLI 默认保留，`--move` 删除。
- **跨进程锁**：边车文件 `{容器规范路径}.lock`（不是对容器本体做字节范围锁，否则 Windows 会挡住本进程自己的读写）。同一进程内 GUI 会话、挂载 writer、短时 reader 共用一把锁（引用计数）；另一进程得到 `ContainerBusy`。
- **浏览**：非空过滤搜索整棵树。容器内复制与移动有目标对话框；复制拒绝已存在的目标和复制到自身内部。添加、读出、删除、复制、移动、校验、碎片整理、恢复在后台线程跑，界面有进度遮罩。
- **未做**：页级差量写（大文件仍是整文件重加密）。macOS Finder 挂载未改。
- **托盘**：关闭或最小化窗口会藏到系统托盘，进程和挂载继续。窗口隐藏期间不自动锁定。图标区分未解锁、已解锁、挂载中。右键：显示窗口；已解锁后可添加文件（进当前目录）、打开挂载位置、锁定；「退出」才结束进程。左键或双击恢复窗口。

### 10.2 WinFsp 写入

先前文档把「写入后读回为空」写成过时的环境问题。本机复验（边车锁已接入）仍然是 3 个写入用例失败、读用例通过：新建文件读回 `[]`，覆盖后容器里仍是原文，`remove_dir` 返回「目录不是空的」。

原因是卷参数没有在最后句柄关闭时刷掉 Windows 缓存。数据停在缓存里，`close` 又可能拖到卸载之后，写缓冲被丢掉。处理：

- `FlushAndPurgeOnCleanup`，让缓存写进 copy-on-write 缓冲之后再结束清理。
- `cleanup` 里提交脏缓冲；待删除的句柄先丢掉缓冲再删除，避免 `close` 把文件写回去。
- `FileInfoTimeout = 0`，新句柄立刻看到提交后的大小。

复验：`cargo test --no-default-features --features mount --test winfsp_mount_tests -- --test-threads=1` → **4 passed / 0 failed**（约 74s）。

### 10.3 其他验证

- `cargo test --features gui-mount --lib`：79 passed（含 rekey、grow、copy、边车锁、读出默认保留的设置缺省）。
- `cargo test --features "cli,mount" --test cli_tests`：8 passed（默认 extract 保留，`--move` 删除）。
- macOS 挂载代码未改。
