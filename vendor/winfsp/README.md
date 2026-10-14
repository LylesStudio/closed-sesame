# WinFsp — Windows File System Proxy

ClosedSesame does **not** bundle the WinFsp installer. Install it from the official project:

<https://github.com/winfsp/winfsp/releases>

## What is it?

WinFsp enables FUSE-style filesystems on Windows. ClosedSesame uses it to mount `.sesame`
containers as writable virtual drives or folders. File updates use a
copy-on-write buffer and are committed when the open handle is flushed or
closed.

## Installation

1. Download the latest **WinFsp 2.x** MSI from the releases page above.
2. Run the installer (administrator required).
3. Select **Core** and **Developer** components.
   - **Core** provides the runtime DLL.
   - **Developer** provides import libraries used when building ClosedSesame with the `mount` feature.

## License

WinFsp is GPLv3 with a FLOSS license exception for linking against official binaries.
See [THIRD_PARTY.md](../../THIRD_PARTY.md) for how this relates to Apache-2.0 distribution of ClosedSesame.
