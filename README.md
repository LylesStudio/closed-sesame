# ClosedSesame 🔒

**Languages:** [English](README.md) | [简体中文](README.zh-CN.md) | [日本語](README.ja.md)

ClosedSesame is an experimental encrypted file container written in Rust. Files live in `.sesame` containers using AES-256-GCM, Argon2id/HKDF, and page-level authenticated encryption.

The project is still early; do not keep your only copy inside a container.

## Current capabilities

Status legend: **implemented** means the code path exists; **build-verified** means the platform has been built/tested in CI; **runtime-verified** means real mount and file access were tested. These are not interchangeable. **Roadmap** means not done yet.

- Container
  - **Available**: V3 containers with self-describing KDF parameters; password and no-password modes.
  - **Available**: Nested paths and directories (`mkdir`, recursive `add --recursive`, recursive delete).
  - **Available**: Authenticated verification, CRC32 file checks, defragmentation, and read-only recovery.
  - **Available**: Three header copies — Header A, Header B, and Footer.
- Frontends
  - **Available**: CLI `create`, `verify`, `list`, `add`, `extract`, `remove`, `defrag`, `mkdir`, `rename`, `passwd`, `grow`, `mount`.
  - **Available** (`gui-mount`): sidebar + breadcrumb browser (obsidian/dark-gold theme), file/folder import (folder name preserved in vault), recursive directory extract with conflict pre-check (no silent overwrite of files/dirs; symlinks cleared), Ctrl/Shift multi-select with batch delete/extract, rename/open (single selection), mount panel, folder symlinks, recent vaults, auto-lock, anti-screenshot (Windows/macOS via winit; Linux best-effort with honest capability notes).
- Mounting
  - **Linux — implemented: yes; build-verified: yes; runtime-verified: yes (read+write).** `fuser` adapter with copy-on-write-on-close writes, two-phase container commits, and live view refresh.
  - **macOS — implemented: yes (WebDAV bridge); build-verified: yes; runtime-verified: protocol-level (raw HTTP integration tests); Finder verification pending.** A loopback WebDAV server (`127.0.0.1` + random session token) that Finder mounts natively — no macFUSE or other external filesystem runtime required.
  - **Windows — implemented: yes; build-verified: yes; runtime-verified: yes (read+write).** `winfsp_wrs` + user-installed WinFsp 2.1. Writes are copy-on-write-on-close; `winfsp_mount_tests` covers create, truncate-overwrite, and mkdir/rename/delete (4/4 on this machine, 2026-10-02).
- Other
  - **Available**: File and folder symlink lifecycle (while mounted + `--symlink` / GUI toggle); CLI `add --recursive` defaults to source folder name when `-n` is omitted; recovery `extract` aligned with GUI conflict/symlink handling; GitHub release workflow (tag `v*`); Apache-2.0 + [THIRD_PARTY.md](THIRD_PARTY.md).

## Internationalization

Runtime UI and CLI strings support **English**, **Simplified Chinese**, and **Japanese**.

- CLI / recover: `--lang en|zh-CN|ja` or `CLOSED_SESAME_LANG`
- GUI: first-run language picker when unset; Settings → Language (persisted under the OS config directory)
- Translations live in [`locales/`](locales/); see [docs/en/CONTRIBUTING-i18n.md](docs/en/CONTRIBUTING-i18n.md)

## CLI

The `sesame` binary requires `cli` and `mount` features:

```bash
cargo build --release --bin sesame --features "cli,mount"
```

Common commands:

```bash
sesame create treasure --size 500MB
sesame create treasure --size 500MB --no-password
sesame add treasure.sesame ./docs --recursive --no-password
sesame list treasure.sesame
sesame mount treasure.sesame /mnt/treasure
sesame --lang zh-CN list treasure.sesame
```

Password prompts are hidden and zeroized after use. Use `--no-password` for no-password containers where supported. After a successful CLI mount, press Enter to unmount.

Windows mounting requires [WinFsp 2.x](https://github.com/winfsp/winfsp/releases) (Core + Developer) installed separately. **macOS mounts via the built-in loopback WebDAV bridge** (`sesame mount --webdav`, default in the GUI on macOS); Finder connects natively, no kernel extension needed. Linux uses the system FUSE stack (`/dev/fuse`) and mounts **writable** by default.

Extract defaults to **copy**: the file stays in the container. `sesame extract --move` removes it after a verified write (the hidden `--keep` flag is a no-op kept for old commands). GUI "Move out" is a separate confirmation; "Open" decrypts to a per-session cache, watches edits, and writes changes back into the vault on close, with timestamped conflict protection. `sesame passwd` changes the password without rewriting file pages (confirm an empty password to remove it); `sesame grow --by 1GB` extends capacity in place (rounded up to 4 MiB blocks). See [STATUS.md](STATUS.md) for the authoritative capability/risk record.

On Windows, the real mount test can be run after installing WinFsp:

```powershell
cargo test --no-default-features --features mount --test winfsp_mount_tests -- --nocapture
```

On Linux, when `/dev/fuse` is available:

```bash
cargo test --no-default-features --features mount --test fuse_mount_tests -- --nocapture
```

See `sesame --help` and `sesame <command> --help` for full options.

## GUI and recovery

```bash
# NOTE: without `gui-mount` the GUI builds without the mount panel.
cargo build --release --bin sesame-gui --features gui-mount
cargo run --release --bin sesame-gui --features gui-mount
```

Closing or minimizing the window leaves the app in the system tray so a mount can keep running. Quit from the tray menu. The GUI uses a sidebar + main-content layout (`views/locked`, `browser`, `settings`). Multi-select (Ctrl/Shift) enables batch delete and extract; rename and open require a single selection. Directory extract preserves vault-relative paths and rejects conflicts without overwriting existing files or folders.

Recovery tool (separate feature):

```bash
cargo build --release --bin sesame-recover --no-default-features --features recover
sesame-recover verify treasure.sesame
sesame-recover extract treasure.sesame --output ./recovered --skip-corrupted
```

## Development

```bash
cargo test
cargo clippy --all-targets
```

See [Architecture](docs/en/ARCHITECTURE.md) and [GUI design](docs/en/DESIGN.md).

## Format compatibility

Only V3 is read and written. V2 containers are rejected with guidance to recreate. `--no-password` uses a deterministic public HKDF — not a substitute for a strong password.

## License

Apache-2.0. Third-party runtime and mount dependencies are documented in [THIRD_PARTY.md](THIRD_PARTY.md). See [CONTRIBUTING.md](CONTRIBUTING.md) and [SECURITY.md](SECURITY.md) before submitting changes or vulnerability reports.
