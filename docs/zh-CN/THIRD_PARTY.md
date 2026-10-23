# Third-Party Notices

See also the root copy at [`../THIRD_PARTY.md`](../THIRD_PARTY.md).

ClosedSesame is distributed under **Apache-2.0**. Runtime dependencies with separate licensing:

- **WinFsp** (Windows mount): GPLv3 + [FLOSS exception](https://github.com/winfsp/winfsp/blob/master/License.txt). Not bundled; install from https://github.com/winfsp/winfsp/releases
- **winfsp_wrs** / **winfsp_wrs_sys** / **winfsp_wrs_build**: MIT
- **FUSE / libfuse3** (Linux mount): LGPL-2.1, system library, not bundled
- **fuser** / **tiny_http** / **httpdate** / **windows-sys** (transitive): MIT OR Apache-2.0
- **notify** (GUI cache watcher): CC0-1.0
- **egui** / **eframe** (GUI): MIT OR Apache-2.0; **rfd**: MIT
- **Source Sans 3** (GUI Latin) and **Source Han Sans / Noto Sans CJK** (SC + JP Regular): SIL OFL 1.1 — `assets/fonts/SourceSans3-Regular.ttf`, `assets/fonts/NotoSansCJKsc-Regular.otf`, `assets/fonts/NotoSansCJKjp-Regular.otf`
- 图标组与应用 logo（`assets/icons/`、`assets/logo-*.svg`）：项目原创，随 Apache-2.0 发布

macOS 挂载使用内置 loopback WebDAV 桥，无需任何外部文件系统运行时（不需要 macFUSE）。

分发 ClosedSesame 二进制时请附带本文件，且不要随应用再分发 WinFsp。
