# GUI design boundaries

**Languages:** [English](../en/DESIGN.md) | [简体中文](../zh-CN/DESIGN.md) | [日本語](../ja/DESIGN.md)

## Information architecture

- **First-run locale** (`views/locale_setup.rs`): shown when no language is persisted; three-button picker before the locked page.
- **Locked page** (`views/locked.rs`): centered card for open/create vault, recent list, password / no-password mode.
- **Browser page** (`views/browser.rs`): left sidebar (vault status, add file/folder, mount controls, lock/settings) + main area with breadcrumb navigation and file list.
- **Settings page** (`views/settings.rs`): grouped toggles for auto-lock, anti-screenshot, WinFsp link, theme, **language selector**.

Visual theme: obsidian/dark-gold dark mode and pearl/slate light mode (`theme.rs`); self-drawn SVG icon set (`assets/icons/`); increased spacing and rounded corners.

## Implemented interactions

- Breadcrumb path browsing (`browser.rs` + `current_dir`)
- File/folder import; folder import preserves the source directory name in the vault
- Single-file and recursive directory extract with conflict pre-check (no silent overwrite)
- **Multi-select**: Ctrl toggle, Shift range, right-click conventions (`selected_paths`, `selection_anchor`)
- **Delete confirmation** (`dialog.rs`): single-item and batch delete both require explicit confirm; batch lists up to five paths
- **Extract info** (`dialog.rs`): optional first-run explanation with “don’t show again”
- **Symlink info** (`dialog.rs`): explains folder-root symlink semantics; optional “don’t show again”
- **Unlock recent** (`dialog.rs`): password prompt when opening a recent vault from the list
- **Batch extract**: one output folder, all-or-nothing pre-check, status message with count
- **Single-select only**: rename, open externally
- Decrypt-to-temp then `open_external`
- Mount/unmount (`gui-mount`) and symlink toggle (requires active mount); supports file and folder symlinks
- Recent vaults (`recent.rs`, JSON in config dir)
- Auto-lock (default 10 minutes, configurable)
- Anti-screenshot (`anti_capture.rs`, Settings toggle, active when unlocked): `ViewportCommand::ContentProtected` → winit `set_content_protected` on Windows/macOS; Linux sends the same command (currently no-op) with environment-specific capability notes (X11 / Wayland / KDE)
- CJK rendering via embedded Noto Sans SC subset (`assets/fonts/`)

## Extract conflict UX

- `ExtractConflict` surfaces in the status bar; user picks a different output folder manually (no auto-reopen of the folder picker).
- `ExtractMountReadOnly` rejects extracting into an active mount path so output is written beside the vault, not through its mounted view.
- Existing symlinks at the destination are cleared silently before write.

## Boundaries

- Anti-screenshot is not a security boundary; Linux has no universal in-app API; KDE users may use window rules as a workaround.
- macOS 15+ ScreenCaptureKit may ignore content protection; anti-capture remains best-effort.
- Mounts are writable (copy-on-write on close) on Linux FUSE and Windows WinFsp. macOS uses the WebDAV bridge; Finder behavior is not verified on a Mac.
- Batch extract does not merge into an existing directory; top-level name conflicts reject the whole operation.

## Build

```bash
cargo run --features gui-mount --bin sesame-gui
```
