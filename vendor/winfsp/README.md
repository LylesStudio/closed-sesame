# WinFsp — Windows File System Proxy

ClosedSesame does **not** bundle the WinFsp installer. Install it from the official project:

<https://github.com/winfsp/winfsp/releases>

## What is it?

WinFsp enables FUSE-style filesystems on Windows. ClosedSesame uses it to mount `.sesame`
containers as writable virtual drives or folders. File updates use a
copy-on-write buffer and are committed when the open handle is flushed or
closed.

## Installation

For users of a prebuilt ClosedSesame release:

1. Download the latest **WinFsp 2.x** MSI from the official [WinFsp release page](https://winfsp.dev/rel).
2. Run the installer and install the **Core** component.
   - Core provides the runtime driver and DLL required for Windows mounting.
   - The **Developer** component is only needed when building ClosedSesame from source.

ClosedSesame release archives do not bundle the WinFsp installer or driver.

For source builds, install the Developer component as well; it provides the import libraries used by the `mount` feature.

## License

WinFsp is GPLv3 with a FLOSS license exception for linking against official binaries.
See [THIRD_PARTY.md](../../THIRD_PARTY.md) for how this relates to Apache-2.0 distribution of ClosedSesame.
