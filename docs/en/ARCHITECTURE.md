# ClosedSesame Architecture

**Languages:** [English](../en/ARCHITECTURE.md) | [简体中文](../zh-CN/ARCHITECTURE.md) | [日本語](../ja/ARCHITECTURE.md)

This document describes the current code. Core principle: container format and use cases must not depend on a specific frontend or mount backend.

## Module boundaries

```text
CLI (`src/cli`) ───────┐
GUI (`src/gui`) ───────┼──> app ──> container ──> crypto / keymgmt
Recovery (`src/recover`)┘       └──> error / i18n

mount (`src/mount`) ──> SharedVault ──> FUSE (read+write) / WebDAV bridge / winfsp_wrs (read+write)
```

> Current capability status, verification state, and risks are tracked in
> [`STATUS.md`](../../STATUS.md) — the authoritative record.

- `app`: use cases (credentials, vault CRUD, recovery, symlink registry).
- `app/vault.rs`: shared extract/add/remove paths; `ensure_extract_destination` pre-checks conflicts before any write.
- `app/symlink.rs`: session registry for file and **directory** symlinks at `original_path` while mounted.
- `container/path.rs`: in-container path normalization (`/` separators, no `..`).
- `mount/vfs.rs`: inode tree from flat `FileEntry` list.
- `mount/shared.rs`: `SharedVault` — VFS snapshot + single writer mutex shared by all mount frontends (FUSE, WebDAV) and out-of-band GUI mutations; reloads the snapshot after each committed write.
- `mount/fuse_state.rs` + `fuse_adapter.rs`: writable FUSE — copy-on-write-on-close buffers committed on flush/release.
- `mount/webdav.rs`: loopback WebDAV bridge (macOS mount path; Finder-native, no macFUSE).
- `container/writer.rs`: two-phase commits (new pages durable before old pages are released) with `sync_data` on every persist step.
- `mount_hints.rs`: WinFsp download link and supplemental error guidance (shared by CLI/GUI).
- `gui/views/`: locked, browser, settings, locale_setup; `gui/dialog.rs` for delete/extract/symlink/unlock modals; `gui/selection.rs` dedupes parent/child paths for batch delete.
- `gui/anti_capture.rs`: `ViewportCommand::ContentProtected` with Linux environment detection for capability notes.
- `i18n`: centralized translations in `locales/*.yml` via `rust-i18n`.

## Directory model

- `FileEntry.path` uses `/`-separated relative paths; `is_directory=true` marks empty directory entries.
- Missing intermediate directories are created on add; `mkdir`, `rename_path`, and recursive `remove_path` live in `ContainerWriter`.
- Folder import (GUI and CLI `--recursive` without `-n`) wraps content under the source folder name.
- `validate_entry_tree` checks for orphans, duplicates, and directories occupying data pages.

## Extract and conflict policy

- `ensure_extract_destination`: allows missing paths and existing symlinks; rejects ordinary files and directories (`ExtractConflict`).
- Rejects extracting into an active mount path (`ExtractMountReadOnly`) to avoid writing the output back through the mounted view.
- Directory extract collects all descendant files, pre-checks every destination (all-or-nothing for batch), then writes with `clear_symlink_for_write`.
- `recovery::extract_all` uses the same helpers so recovery and GUI/CLI extract behave consistently.

## Mounting

- Linux: shared `fuser` adapter (writable, copy-on-write-on-close). macOS: built-in loopback WebDAV bridge (Finder-native, no external runtime). The macFUSE/FSKit adapter is removed; `mount_vault` on macOS fails and points at `sesame mount --webdav`.
- Windows: `winfsp_wrs::FileSystemInterface` with segment-by-segment lookup and subdirectory enumeration.
- `mount::check_prerequisites()` runs before mount; missing WinFsp errors include the official releases URL.

## Symlinks

- Enabled only while the vault is mounted and the user opts in (`--symlink` / GUI toggle).
- **Files**: `move_with_symlink` replaces the source file with a symlink to `{mount_point}/{container_path}`.
- **Folders**: `move_dir_with_symlink` removes the source tree (content already in vault) and places a directory symlink at the original path.
- Registry entries are removed on lock/unmount; `restore_symlinks` recreates links after remount.

## Frontends

- CLI: `create`, `verify`, `list`, `mount`, `defrag`, `add` (`--recursive`, `--symlink --mount-point`), `extract`, `remove`, `mkdir`, `rename`.
- GUI (`gui-mount`): sidebar layout, breadcrumb browser, multi-select (Ctrl/Shift), batch delete/extract with confirmation dialogs, single-select rename/open, mount panel, folder symlinks, recent vaults, auto-lock, anti-capture (Windows/macOS via winit; Linux best-effort), first-run locale picker, language selector.
- `sesame-recover`: read-only verify/list/extract preserving nested directories and the same extract conflict rules.

## Verification status

- **Build-verified**: Windows/Linux CI covers `cli,mount,gui-mount,recover` without installing WinFsp.
- **Runtime mount**: Linux runs `fuse_mount_tests` when CI provides `/dev/fuse`. Windows end-to-end mount and directory-symlink tests (`winfsp_mount_tests`, `winfsp_symlink_tests`) require a locally installed WinFsp. macOS mounts through the loopback WebDAV bridge (`webdav_tests` cover the protocol). The macFUSE/FSKit path is removed. Finder behavior on a real Mac is not yet verified.
