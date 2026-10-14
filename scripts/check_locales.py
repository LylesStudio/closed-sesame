#!/usr/bin/env python3
"""Check that every supported locale has the same top-level message keys."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

LOCALE_FILES = ("en.yml", "zh-CN.yml", "ja.yml")
KEY_PATTERN = re.compile(r"^([^:#\s][^:]*):")


def locale_keys(path: Path) -> set[str]:
    keys: set[str] = set()
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("_"):
            continue
        match = KEY_PATTERN.match(line)
        if match:
            keys.add(match.group(1).strip())
    return keys


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("locales_dir", nargs="?", type=Path, default=Path("locales"))
    args = parser.parse_args()

    keys_by_locale: dict[str, set[str]] = {}
    for filename in LOCALE_FILES:
        path = args.locales_dir / filename
        if not path.is_file():
            print(f"Missing locale file: {path}", file=sys.stderr)
            return 1
        keys_by_locale[filename] = locale_keys(path)

    baseline = keys_by_locale["en.yml"]
    failed = False
    for filename in LOCALE_FILES[1:]:
        current = keys_by_locale[filename]
        missing = sorted(baseline - current)
        extra = sorted(current - baseline)
        if missing or extra:
            failed = True
            print(f"Locale parity failed for {filename}")
            if missing:
                print("  Missing keys:")
                print("".join(f"    {key}\n" for key in missing), end="")
            if extra:
                print("  Extra keys:")
                print("".join(f"    {key}\n" for key in extra), end="")

    if failed:
        return 1
    print(f"Locale key parity OK ({len(baseline)} keys x {len(LOCALE_FILES)} locales)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
