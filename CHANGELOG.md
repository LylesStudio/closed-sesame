# Changelog

All notable changes to this project will be documented here.

## Unreleased

### 2026-10-23 — Tray flyout, embedded fonts, release script

#### Added

- Windows tray flyout panel with per-item actions. It dismisses as soon as it
  loses focus and does not reopen on the tray click that caused the blur.
- `scripts/build-release.sh` for one-click macOS and Linux release builds,
  building the same feature sets as the Windows script and CI.

#### Changed

- GUI fonts: Source Sans 3 for Latin, plus Source Han Sans / Noto Sans CJK
  (Simplified Chinese and Japanese) for Han and kana. These replace the
  Noto Sans SC subset, so Japanese UI text is no longer drawn from a
  Simplified Chinese face.
- Tray and mount strings localized in English, Japanese, and Simplified Chinese.

#### Fixed

- `NOTICE` now attributes the fonts that release binaries actually bundle.
- Removed the promo-video README placeholder links, which pointed at release
  assets that were never published.

### 2026-10-14 — Public release hardening

#### Added

- Public-release security policy, contributor guide, code of conduct, and the
  complete SIL Open Font License 1.1 text for the bundled fonts.
- Portable locale parity check at `scripts/check_locales.py`.
- GitHub Actions quality and tag-driven release workflows under
  `.github/workflows/`.

#### Fixed

- Applied `cargo fmt` to the 32 files that were failing the repository format
  gate and cleared the Clippy errors in the active Linux/CLI/mount/recovery
  feature set, including platform-gated dead code and test-only warnings.
- Split the duplicate i18n workflow command, removed the obsolete macFUSE
  runner label, and run WebDAV tests on the macOS mount job.
- Aligned mount and extraction documentation with the writable Linux, Windows,
  and WebDAV backends.

### 2026-10-02 — Vault maintenance

#### Added

- **Change password and grow in place.** `sesame passwd` and the GUI rewrap the master key (file pages stay as they are). Leaving the new password empty removes it. `sesame grow --by SIZE` and the GUI extend the bitmap and footer by whole 4 MiB blocks. Both refuse to run while the vault is mounted.
- **Maintenance in the GUI.** Settings can verify, defragment, recover into a chosen folder, change the password, and grow the container. Verify, defrag, recover, import, extract, delete, copy, and move run on a background thread with a progress overlay.
- **Vault-wide search, in-container copy and move.** A non-empty browser filter searches the whole tree. Copy and move ask for a destination inside the vault; copy refuses an existing destination and a copy into itself.
- **Cross-process lock.** Opening a container takes a sidecar lock (`{path}.lock`) with an in-process refcount, so a GUI session and its mount share one lock while another process gets `ContainerBusy`.

#### Changed

- **Close and minimize hide the window in the system tray.** The process and any mount keep running; auto-lock pauses until the window is shown again. The icon shows locked, unlocked, or mounted. The menu can show the window, add files into the current folder, open the mount, lock, or quit. Left-click or double-click restores the window.
- **Extract keeps the vault copy.** GUI extract no longer deletes the source. "Move out" is a separate confirmed action. CLI `extract` keeps the file unless `--move`; hidden `--keep` is accepted and does nothing. The settings toggle is gone; `keep_files_on_extract` remains in `settings.json` only so older files still parse (default true).

#### Fixed

- **WinFsp writes were not durable.** The volume cached writes and did not flush them before the last handle closed, so a new file read back empty, an overwrite left the original bytes, and removing a directory failed with "directory not empty". Cleanup now commits the copy-on-write buffer, and the volume flushes and purges the cache on cleanup. `winfsp_mount_tests` 4/4 on Windows.

### 2026-10-02 — GUI interaction redesign

#### Added

- **GUI interaction redesign.** `Theme` (persisted choice: Cave / Parchment) cross-fades into a per-frame `Palette`; every widget paints from the palette. Locked page: breathing glow, rotating seal ticks, error shake, Enter-to-submit, recent vaults with relative time, password-strength meter, and capacity presets. Browser: breadcrumbs, in-folder search, sortable columns (name / size / modified; directories first, natural order), inline rename, keyboard navigation, drag-and-drop overlay, empty-folder drop target, sidebar capacity bar and mount status, animated notification panel. Settings: theme preview swatches, language segment control, auto-lock toggle and slider. Dialogs fade and slide in. New `gui.*` / `gui.time.*` keys in en, zh-CN, and ja.
- Debug `CLOSED_SESAME_DEMO` accepts composable sub-states for screenshots: `create`, `error`, `settings`, `select`, `delete`, `search`, `rename`, `mkdir`, `bell` (for example `dark-locked-create`, `light-select`).
- Light theme retuned from gray-white + silver to warm parchment + aged brass, sharing the dark theme's metal.

#### Fixed

- Theme and auto-lock were never written to `settings.json`.
- Context-menu Rename committed the original name with no editor. Shift range selection followed name order instead of the visible sort order. Deleting the current directory left breadcrumbs on a path that no longer exists (falls back to the nearest existing parent).
- An empty folder name could be submitted. Creating a vault with an empty password only failed after the save dialog.
- Auto-lock idle time reset only on clicks, so reading or scrolling could lock the vault. Any pointer, scroll, or key activity now counts.
- Opening a file on Windows flashed a console window (`CREATE_NO_WINDOW`).
- Near-black glow meshes speckled under egui's default gradient dithering (`NativeOptions::dithering = false`; glow rings 6 → 16). The light theme `accent_glow` was an invalid premultiplied color.
- A `/dev/fuse` platform error on Windows incorrectly appended the WinFsp download URL. The URL is now chosen from the failing runtime, so FUSE guidance has no download link (`mount_hints`).

### Added

- **Writable WinFsp mount (Windows)**: full write callbacks (`create_ex/open/overwrite_ex/write/flush/close/cleanup/set_file_size/set_basic_info/set_delete/rename`) on `SharedVault` with copy-on-write-on-close commits; read-only volume flag and READONLY attributes removed; `write_buffer` unit tests and `winfsp_mount_tests` write scenarios (create, truncate-overwrite, mkdir/rename/delete).
- **Writable FUSE mount (Linux)**: copy-on-write-on-close writes (`create/open/write/setattr/mkdir/unlink/rmdir/rename/flush/fsync/release`) with two-phase container commits; live VFS refresh (`refresh_vault` now implemented for FUSE backends); real-mount write tests in `fuse_mount_tests`.
- **macOS loopback WebDAV bridge**: `src/mount/webdav.rs`, `mount_webdav_with_kek`, CLI `sesame mount --webdav`, GUI mount on macOS uses it with Finder auto-mount; raw-HTTP integration tests (`tests/webdav_tests.rs`).
- **Cache write-back for GUI "Open"**: per-session cache dir (`notify` watcher, timestamp-prefixed names), debounce write-back via `ContainerWriter::replace_file_data`, conflict dialog (overwrite / keep-both with timestamped rename / discard), lock/exit flush with zeroize-and-delete cleanup.
- **Extract**: originally shipped as move-on-extract (`extract --keep` to copy). Superseded the same day: extract now keeps the file unless `extract --move`.
- Container write robustness: two-phase commit for add/replace/remove, rollback on page-write failure, `sync_data` on all persist steps, `replace_file_data` preserving entry identity metadata.
- GUI notifications: auto-expiring toast (5s + fade), bell icon with unread badge, and a notification history panel (clear-all supported).
- Self-drawn SVG icon set (`assets/icons/`, 20 stroke icons tinted per theme) replacing the Lucide font; new flat logo (`assets/logo-mark.svg` + `assets/logo-icon.svg`) used for the window icon and the Windows .exe resource icon (`embed-resource` in `build.rs`).
- Windows GUI: `windows_subsystem = "windows"` (no console window); settings shows real WinFsp detection status; mount panel shows an explicit hint when the build lacks the `gui-mount` feature.
- Layout: design-canvas scale clamping (`MAX_SCALE`), larger minimum window size, settings page scrolls instead of clipping (letterbox accepted, never cut off).
- Usability: double-click a file to open it, "Open mount location" button after mounting, selection action bar (open/extract/rename/delete).

### Fixed

- Icons rendered as red error triangles: egui 0.34 loads images via `egui_extras` loaders, which were never installed; added `egui_extras` with the `image` feature and `install_image_loaders` at startup.
- Layout reworked to fully responsive panels (no fixed design canvas, no letterboxing): locked/open-vault page and first-run locale page rewritten with adaptive centered layout, settings scrolls, dialogs clamp to viewport, window `min_inner_size` raised to 560×420.
- Locked page recent-vault list is now capped with its own scroll area so the card fits the default 960×640 window.
- Extract info dialog text updated to move semantics (was still describing copy semantics) in en/zh-CN/ja.
- Mount startup latency: `validate_entry_tree` rewritten from O(n²) pairwise scan to O(n·depth) hash join (runs on every container open), and symlink-restore no longer re-reads the full file table when the registry is empty.
- Cache write-back hardening: reopening a file flushes pending edits then rebuilds the cache copy from the vault; when mounted with a local mount point, GUI "Open" goes through the mount path instead of the cache; conflict detection reads the live `modified_at` from the container so edits made via a mount are correctly flagged and saved as timestamped conflict copies.
- Recent-vault store: atomic saves (temp + rename), corrupt-store quarantine with stderr logging instead of silently returning an empty list, and round-trip tests.
- GUI redesign: obsidian+gold (dark) and gray-white+silver (light) themes with `accent_soft`/`primary_fill`/`accent_text` tokens, brand hero on the locked page, unlocked pill, size column, accent-edge toasts; debug-only `CLOSED_SESAME_DEMO` seeding for screenshots.
- `STATUS.md` as the authoritative capability/risk record.

### Changed

- Fixed pre-existing bugs: `DesignCanvas::inner_rect` missing origin offset (layout shifted on non-default window sizes), FUSE `refresh_vault` stub (stale mounted views after GUI mutations), case-insensitive `path_is_within_mount` on Linux, eframe missing `wayland` feature.
- macFUSE fully removed: the dormant FSKit adapter code (`src/mount/macos.rs`), third-party notices, user-facing install guidance, CI setup script, and docs all deleted; `fuser` is now Linux-only; macOS mounting is served exclusively by the built-in WebDAV bridge. `mount_supported()` now reports true on macOS (WebDAV), fixing the mount panel being hidden there. Repository prepared for Apache-2.0 open-sourcing: `NOTICE` file added, `THIRD_PARTY.md` updated (new deps: tiny_http, httpdate, notify, egui/eframe/rfd), stale internal plan docs removed.

- GUI multi-select (`selected_paths`, Ctrl/Shift) with batch delete and batch extract; rename and open remain single-selection.
- Delete confirmation dialog for single and batch vault removal (`gui/dialog.rs`); symlink cleanup on delete.
- Extract and symlink info dialogs with optional “don’t show again”; unlock-recent password dialog.
- First-run locale picker (`views/locale_setup.rs`) when no language is persisted.
- Recursive directory extract in vault/CLI/GUI; `ensure_extract_destination` conflict pre-check (`ExtractConflict`) and `ExtractMountReadOnly` with all-or-nothing batch semantics.
- Folder import preserves source directory name (GUI and CLI `add --recursive` without `-n`).
- Directory symlink support (`move_dir_with_symlink`); Windows `winfsp_symlink_tests` end-to-end coverage when WinFsp and symlink permissions are available.
- `gui/selection.rs` for parent/child path dedupe on batch delete.
- `gui/views/` module split (`locked`, `browser`, `settings`) with obsidian/dark-gold theme refresh.
- Recovery `extract_all` aligned with GUI extract (conflict pre-check + `clear_symlink_for_write`).
- Runtime i18n for en / zh-CN / ja (`locales/`, `rust-i18n`); locale parity checked in CI via `scripts/check_locales.py`.
- `container/path.rs`, `app/symlink.rs`, `mount_hints.rs`, release workflow (`.gitea/workflows/release.yml`).
- Tests: `path_tree_tests`, `mount_hints_tests`, `i18n_tests`, `cli_tests`, `recover_tests`, nested VFS unit tests, `winfsp_mount_tests`.

### Changed

- **macOS mount:** temporarily disabled at runtime (`MACOS_MOUNT_ENABLED = false`) due to macFUSE FSKit instability on recent macOS versions; GUI mount panel and CLI `mount` fail fast with a localized message; docs and CI updated accordingly. FSKit adapter code remains for future re-enable.
- **macOS mount (prior):** macFUSE 5.2+ FSKit backend (`backend=fskit`, user-space); mount points under `/Volumes/`; updated prerequisites, docs, CI (`macfuse` runner label), and conditional `fuse_mount_tests` on macOS.
- **License:** unified to Apache-2.0 only; added `LICENSE` and `THIRD_PARTY.md`.
- **Windows mount:** migrated from `winfsp` (GPL crate) to `winfsp_wrs` (MIT); delay-load via `winfsp_wrs_build`.
- Removed bundled `vendor/winfsp/winfsp-2.1.msi`; users install WinFsp from official GitHub releases.
- Mount/VFS supports hierarchical paths and directory entries end-to-end (container, defrag, recover, FUSE, WinFsp).
- GUI refactor: sidebar + breadcrumb browser, file management, auto-lock, recent vaults, anti-capture (Windows/macOS via winit), mount panel (`gui-mount` feature).
- CLI: `mkdir`, `rename`, `add --recursive`, `add --symlink --mount-point`.
- Upgraded the active container format to **V3**. Three 256B headers persist big-endian KDF algorithm/schema, memory, iterations, parallelism, and output length; opening derives credentials strictly from those fields.
- **Breaking:** V2 containers are rejected with `UnsupportedVersion`; extract with an older build and recreate as V3. No in-place V2 migration.
- Password containers default to Argon2id (64 MiB, 3 iterations, parallelism 1, 32-byte output). A release-build derivation measured ~78.7 ms on Windows x86_64 (Intel Core i7-14700K, 2026-07-11).
- CLI and recovery password prompts no longer echo input; transient password strings are zeroized; GUI lock/drop paths clear passwords.
- Routed CLI and GUI container operations through shared `app` use cases.
- Replaced platform-specific mount code with `mount_vault`/`MountedVault`, shared Linux/macOS `fuser` adapter, and native Windows WinFsp callbacks.
- GUI anti-capture unified via `ViewportCommand::ContentProtected` (Windows + macOS); setting persisted in `settings.json`; Linux capability notes by session type.
- Lucide icon font embedded in GUI (`assets/fonts/lucide.ttf`).
- **Breaking:** Replaced `bincode 1.3.3` with Postcard for V3 file-table and allocation-bitmap metadata.

### Removed

- Flat-only mount restriction; root-only file assumption in VFS validation.
- `raw-window-handle` from the `gui` feature (anti-capture no longer calls Win32 APIs directly).

### Known limitations

- macOS kernel FUSE (macFUSE/FSKit) is removed. Mounting uses the loopback WebDAV bridge; `webdav_tests` cover the protocol. Finder behavior on a real Mac is not yet verified.
- Linux has no universal in-app anti-capture API (X11/Wayland); the toggle is kept for future winit/compositor support.
- macOS 15+ ScreenCaptureKit may ignore content protection; anti-capture remains best-effort only.
- No-password mode remains reproducible from public format information and does not resist offline attack.
- Linux real-mount tests (`fuse_mount_tests`) run when `/dev/fuse` is available. Windows real-mount tests require a locally installed WinFsp.
- `cargo audit` may report transitive maintenance warnings (e.g. RUSTSEC-2024-0436 via legacy `paste`); tracked without advisory ignores where no safe upgrade exists.
