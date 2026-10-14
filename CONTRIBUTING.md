# Contributing to ClosedSesame

Thank you for helping make the vault safer and easier to use. Keep changes
small, source-grounded, and easy to review.

## Development environment

- Rust toolchain: the repository-pinned version in `rust-toolchain.toml`
- Python 3 for the portable locale check
- Linux FUSE development packages for Linux mount work
- WinFsp Core + Developer for Windows mount work

Do not commit real vaults, passwords, tokens, private paths, production logs,
or generated release output.

## Before opening a change

Run the checks relevant to your change, preferably all of these from a clean
working tree:

```bash
cargo fmt --all -- --check
python3 scripts/check_locales.py
env -u DATABASE_URL -u REALM_DATABASE_URL -u REALM_DB_URL \
  cargo clippy --all-targets --locked \
  --no-default-features --features cli,mount,recover -- -D warnings
env -u DATABASE_URL -u REALM_DATABASE_URL -u REALM_DB_URL \
  cargo test --locked --all-features
```

If you are on Windows, use `python scripts/check_locales.py` and run the
platform-specific WinFsp tests on a machine with WinFsp installed. If `/dev/fuse`
is unavailable, the real Linux mount test is intentionally skipped; report that
boundary rather than treating a unit test as a mount acceptance test.

## Pull requests

Describe the user-visible behavior, the files changed, the commands run, and
any platform or runtime boundary that was not exercised. Update the relevant
README language copies, `STATUS.md`, `CHANGELOG.md`, and third-party notices
when behavior or distribution changes.

Security-sensitive changes should use the private reporting path in
[`SECURITY.md`](SECURITY.md) instead of a public issue.
