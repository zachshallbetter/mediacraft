# Security Policy

## Reporting a vulnerability

If you find a security issue in MediaCraft, please report it privately rather than opening a
public issue. Use GitHub private vulnerability reporting:

<https://github.com/zachshallbetter/mediacraft/security/advisories/new>

Include:

- a description of the issue and its impact,
- steps to reproduce (a minimal example is ideal),
- the affected component and version or commit.

This is a solo-maintained project. Handling is best-effort with no SLA: there is no
guaranteed response or fix time. Please give a reasonable window to ship a fix before any
public disclosure.

## Scope

MediaCraft is a Rust workspace of 22 crates: a desktop app, a transcode daemon with a typed
IPC protocol, a catalog, and media, document and archive parsers (`craft-archive`,
`craft-pdf`, `craft-psd`, `craft-isobmff`, `craft-matroska` and the codec crates). Its
README treats user files and IPC messages as hostile input and forbids `unsafe` code.

Reports in scope include: memory-safety or panic behavior on crafted media, document or
archive input; resource-budget bypass (decompression limits); path traversal, including
Zip Slip, in `craft-archive`; and unauthenticated or malformed IPC messages that affect
the daemon.

## Supported versions

MediaCraft is at workspace version 0.1.0 and has no tagged releases. Only the latest commit
on `main` receives fixes.
