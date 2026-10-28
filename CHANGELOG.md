# Changelog

All notable changes to this project are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the
project uses [Semantic Versioning](https://semver.org/) (pre-1.0: minor = feature, patch = fix/docs).

## [Unreleased]

## [0.1.0] - 2026-10-27

### Added
- `mnnn` CLI with `~/.muninn` home, preferences and global flags.
- SQLite store with migrations, sources and symbols.
- CSV ingest with source/symbol/duration derivation, validation and idempotent upserts.
- Export to CSV, Parquet and JSON.
- Master key, 24 recovery words, 3-of-5 shards, service keys and SQLCipher encryption.
- Backup and restore, optionally age-encrypted.
- Terminal UI with navigator, ingest, export, keys and theme screens.
- Seven built-in themes with dark/light variants and system mode detection.
- Searchable help built from `docs/`, `mnnn help` and man pages.
- JSON ingest and service ingest from HTTP, Redis, SQLite and Postgres.

[Unreleased]: https://github.com/kommandosaldat/muninn/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/kommandosaldat/muninn/releases/tag/v0.1.0
