# Security policy

ClosedSesame is an experimental encrypted file container. Security reports are
welcome, especially reports involving container confidentiality, integrity,
password handling, path traversal, mount authorization, cache cleanup, or
release artifacts.

## Reporting a vulnerability

Please do **not** open a public issue for an unpatched vulnerability or attach a
real `.sesame` container, password, key, personal data, or private logs.

Use GitHub's private vulnerability reporting form:

<https://github.com/Silver-Aurora/closed-sesame/security/advisories/new>

If private reporting is unavailable, contact the maintainer through a private
GitHub channel and include only the minimum reproducible information. We will
acknowledge a report when we can, investigate it, and coordinate a fix and
public disclosure timeline with the reporter.

## Supported versions

| Version | Support |
|---------|---------|
| `main` | Best effort during active development |
| `v0.1.x` | Security fixes for confirmed high-impact issues |

Older snapshots are not security-supported. The project is experimental; keep
independent backups and do not treat an early release as a replacement for a
well-tested backup strategy.

## Safe report contents

Include the affected version or commit, operating system, feature flags, a
minimal synthetic reproducer, expected behavior, actual behavior, and impact.
Redact passwords, tokens, private paths, personal data, and real vault contents.
