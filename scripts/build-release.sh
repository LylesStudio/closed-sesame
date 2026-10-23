#!/usr/bin/env bash
# One-click local release build for macOS and Linux.
# Matches scripts/build-release.ps1 for the host platform:
#   sesame         --features cli,mount
#   sesame-recover --features recover
#   sesame-gui     --features gui-mount
# Each binary is built separately so feature sets stay the same as CI.
#
# Usage (from the repo root or anywhere):
#   ./scripts/build-release.sh
#   ./scripts/build-release.sh --version 0.1.0
#
# Windows: scripts\build-release.bat

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

version=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      if [[ $# -lt 2 || -z "${2:-}" ]]; then
        echo "error: --version requires a value" >&2
        exit 1
      fi
      version="$2"
      shift 2
      ;;
    -h|--help)
      sed -n '2,14p' "$0"
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      echo "usage: $0 [--version <semver>]" >&2
      exit 1
      ;;
  esac
done

case "$(uname -s)" in
  Darwin|Linux) ;;
  *)
    echo "error: this script builds macOS and Linux releases. On Windows use scripts/build-release.bat." >&2
    exit 1
    ;;
esac

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo was not found on PATH. Install the Rust toolchain from rust-toolchain.toml first." >&2
  exit 1
fi
if ! command -v rustc >/dev/null 2>&1; then
  echo "error: rustc was not found on PATH. Install the Rust toolchain from rust-toolchain.toml first." >&2
  exit 1
fi

if [[ -z "$version" ]]; then
  version="$(awk -F'"' '/^version[[:space:]]*=[[:space:]]*"/ { print $2; exit }' Cargo.toml)"
  if [[ -z "$version" ]]; then
    echo "error: could not read package version from Cargo.toml. Pass --version explicitly." >&2
    exit 1
  fi
fi

# Do not exit awk early: with pipefail, rustc then dies on SIGPIPE and the script aborts before any output.
target="$(rustc -vV | awk '/^host: / { print $2 }')"
if [[ -z "$target" ]]; then
  echo "error: could not read the host target from 'rustc -vV'." >&2
  exit 1
fi

dist_dir="$repo_root/dist"
staging_name="closed-sesame-${version}-${target}"
staging_dir="$dist_dir/$staging_name"
archive_name="${staging_name}.tar.gz"
archive_path="$dist_dir/$archive_name"
checksum_path="$dist_dir/SHA256SUMS.txt"

echo "ClosedSesame release"
echo "  version : $version"
echo "  target  : $target"
echo "  output  : $archive_path"

build_one() {
  local bin="$1"
  local features="$2"
  echo
  echo ">> cargo build --release --locked --no-default-features --features $features --bin $bin"
  cargo build --release --locked --no-default-features --features "$features" --bin "$bin"
}

build_one sesame "cli,mount"
build_one sesame-recover recover
build_one sesame-gui gui-mount

rm -rf "$staging_dir"
mkdir -p "$staging_dir"

release_dir="$repo_root/target/release"
for bin in sesame sesame-recover sesame-gui; do
  src="$release_dir/$bin"
  if [[ ! -f "$src" ]]; then
    echo "error: expected binary was not produced: $src" >&2
    exit 1
  fi
  cp "$src" "$staging_dir/$bin"
done

copy_doc() {
  local source_rel="$1"
  local dest_name="$2"
  local src="$repo_root/$source_rel"
  if [[ ! -f "$src" ]]; then
    echo "error: missing release document: $source_rel" >&2
    exit 1
  fi
  cp "$src" "$staging_dir/$dest_name"
}

copy_doc README.md README.md
copy_doc THIRD_PARTY.md THIRD_PARTY.md
copy_doc LICENSE LICENSE
copy_doc vendor/winfsp/README.md WINFSP-INSTALL.md

rm -f "$archive_path"
# macOS bsdtar otherwise stores AppleDouble files in the archive.
COPYFILE_DISABLE=1 tar -czf "$archive_path" -C "$dist_dir" "$staging_name"

if command -v shasum >/dev/null 2>&1; then
  checksum="$(shasum -a 256 "$archive_path" | awk '{ print $1 }')"
elif command -v sha256sum >/dev/null 2>&1; then
  checksum="$(sha256sum "$archive_path" | awk '{ print $1 }')"
else
  echo "error: neither shasum nor sha256sum was found." >&2
  exit 1
fi

printf '%s  %s\n' "$checksum" "$archive_name" > "$checksum_path"

echo
echo "Release package ready"
echo "  archive : $archive_path"
echo "  sha256  : $checksum"
echo "  sums    : $checksum_path"

list_staged() {
  local path size
  while IFS= read -r path; do
    if [[ "$(uname -s)" == Darwin ]]; then
      size="$(stat -f '%z' "$path")"
    else
      size="$(stat -c '%s' "$path")"
    fi
    printf '  %12s  %s\n' "$size" "$(basename "$path")"
  done < <(find "$staging_dir" -maxdepth 1 -type f | sort)
}

list_staged
