# Third-Party Notices

ClosedSesame is distributed under the **Apache License 2.0** (see `LICENSE` and
`Cargo.toml`). This file summarizes runtime and build-time dependencies that
affect redistribution. See also `NOTICE`.

## WinFsp (Windows mount runtime)

Windows mounting requires [WinFsp](https://github.com/winfsp/winfsp) to be installed separately.
WinFsp is licensed under **GPLv3**, with a [FLOSS License Exception](https://github.com/winfsp/winfsp/blob/master/License.txt)
that permits linking proprietary or Apache-licensed applications against the official WinFsp
binaries without triggering copyleft on the application itself, provided the exception conditions are met.

ClosedSesame does **not** redistribute WinFsp. Users obtain it from the official releases:

<https://github.com/winfsp/winfsp/releases>

Install **Core** and **Developer** components so the runtime DLL and development import libraries are available.

## Mount backends

| Component | License | Notes |
|-----------|---------|-------|
| [FUSE / libfuse3](https://github.com/libfuse/libfuse) (Linux) | LGPL-2.1 | System library; not bundled |
| [fuser](https://crates.io/crates/fuser) | MIT OR Apache-2.0 | Rust FUSE adapter (Linux mount) |
| [tiny_http](https://crates.io/crates/tiny_http) | MIT OR Apache-2.0 | Loopback WebDAV bridge HTTP server (macOS mount) |
| [httpdate](https://crates.io/crates/httpdate) | MIT OR Apache-2.0 | HTTP date formatting for WebDAV |
| [notify](https://crates.io/crates/notify) | CC0-1.0 | Filesystem watcher for the GUI cache write-back |

macOS mounting needs **no external filesystem runtime**: the built-in loopback
WebDAV bridge is mounted natively by Finder.

## Rust crates (Windows mount)

| Crate | License | Role |
|-------|---------|------|
| [winfsp_wrs](https://crates.io/crates/winfsp_wrs) | MIT | Safe Rust bindings to WinFsp |
| [winfsp_wrs_sys](https://crates.io/crates/winfsp_wrs_sys) | MIT | Low-level FFI (build-time registry lookup for import libs) |
| [winfsp_wrs_build](https://crates.io/crates/winfsp_wrs_build) | MIT | MSVC delay-load link directives in `build.rs` |
| [windows-sys](https://crates.io/crates/windows-sys) (transitive) | MIT OR Apache-2.0 | Windows API types used by `winfsp_wrs` |

These crates are compatible with Apache-2.0 distribution of ClosedSesame itself.

## GUI framework crates

The GUI is built on [egui](https://crates.io/crates/egui) /
[eframe](https://crates.io/crates/eframe) (MIT OR Apache-2.0) and
[rfd](https://crates.io/crates/rfd) (MIT) for native file dialogs.

## GUI embedded fonts

Latin text uses [Source Sans 3](https://github.com/adobe-fonts/source-sans) Regular
(`assets/fonts/SourceSans3-Regular.ttf`).
CJK text uses [Source Han Sans](https://github.com/adobe-fonts/source-han-sans)
(Google name: Noto Sans CJK), Regular weight:

- `assets/fonts/NotoSansCJKsc-Regular.otf` — Simplified Chinese
- `assets/fonts/NotoSansCJKjp-Regular.otf` — Japanese

Source Sans 3 is tried first, so it draws English. It has no Han or kana, so those
fall through to the CJK face. The Japanese face is preferred while the UI language
is Japanese; otherwise the Simplified Chinese face is preferred.

All three fonts are licensed under the **[SIL Open Font License 1.1](https://scripts.sil.org/OFL)**.
The complete license text is included at [`assets/fonts/OFL-1.1.txt`](assets/fonts/OFL-1.1.txt).
Source Sans 3: Copyright 2010-2023 Adobe, with Reserved Font Name "Source".
Source Han Sans / Noto Sans CJK: Copyright 2014-2021 Adobe, with Reserved Font Name "Source".
The font files are bundled in release binaries that include the `gui` feature.

## Icons and logo

The toolbar icon set (`assets/icons/`) and the application logo (`assets/logo-*.svg`)
are **original artwork** created for ClosedSesame and are covered by the project's
Apache-2.0 license. No third-party icon font is bundled.
