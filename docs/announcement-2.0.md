# Muninn 2.0

![Three pipeline views as three live columns: price, volume and the tape](../site/web/screenshots/pipeline-live.png)

Muninn (`mnnn`) is a local, encrypted workbench for market data. It ingests bars and
records from files and services, keeps them in a SQLCipher database on your machine, and
gives you a terminal UI and a CLI to search, journal, model and share them. Muninn 2.0
is the result of twenty phases of work, released as v2.0.0 through v2.16.0. Here is
what it brings.

## Research, kept together

- **Authors and action history**: every import, export and change is saved with who did it; step back, forward and fork.
- **Live searches, projects and journals**: saved questions that rerun on new data, grouped with their results and Markdown or LaTeX notes.
- **Reports**: sections built from records, journals and projects, exported to HTML, PDF, Word, LaTeX, JSON or CSV.

![Journals with their entries](../site/web/screenshots/journals.png)

## Rules, models and code

- **Rule sets**: when something happens, or on a schedule (`--every 6h`, `--cron`), run searches, exports, entries or notes.
- **Model Designer and templates**: design record tables, migrate them with previews and roll back; export, import and share designs.
- **Code spaces**: math, Polars, Rust or Python over your data, compiled to native plugins. Python is parsed and translated in Rust, no libpython needed.
- **Pipes and pipelines**: run any data, standard input included, through code spaces and filters. Saved pipelines end in model tables, are designed node by node, and are watched live in tiles.

![The Pipeline Designer](../site/web/screenshots/pipeline-designer.png)

![A code space's result as a live chart tile](../site/web/screenshots/code-chart.png)

## A network you own

- **Peer to peer**: a node over TCP and QUIC with Noise encryption, local discovery, relays and hole punching. Key exchanges happen automatically when you connect.
- **Users, connections and linked-node sync**: one identity across your nodes, all your data on all of them, end to end encrypted.
- **Dark shards**: your master key split among trusted peers, sealed so they cannot read it.
- **Share and reviews**: send any item to a project, journal, code space or connection; review it with comments, suggestions, highlights and votes, on the reviewer's own node.
- **Organizations**: roles, members and hosted HTTPS sites (HTML, JSON and gRPC), with your own domains and Let's Encrypt certificates that renew themselves.

![Peers: nodes seen nearby and through relays](../site/web/screenshots/peers.png)

![Reviews: comments, suggestions, highlights and votes](../site/web/screenshots/reviews.png)

## Install

Every package ships a prebuilt `mnnn` binary; no Rust is needed. Downloads and
`SHA256SUMS` are on the [v2.16.0 release](https://github.com/WeirdBrains/muninn/releases/tag/v2.16.0)
(full steps in [install](install.md)).

| Platform | Download | Install |
|----------|----------|---------|
| Linux or macOS (tarball) | `muninn-2.16.0-<target>.tar.gz` for `x86_64`/`aarch64-unknown-linux-gnu` and `x86_64`/`aarch64-apple-darwin` | `tar -xzf …` then `install -Dm755 muninn-2.16.0-<target>/mnnn ~/.local/bin/mnnn` |
| Debian / Ubuntu | `muninn_2.16.0-1_amd64.deb`, `muninn_2.16.0-1_arm64.deb` | `sudo apt install ./muninn_2.16.0-1_amd64.deb` |
| Fedora | `muninn-2.16.0-1.x86_64.rpm`, `muninn-2.16.0-1.aarch64.rpm` | `sudo dnf install ./muninn-2.16.0-1.x86_64.rpm` |
| Arch Linux | `muninn-bin-2.16.0-1-x86_64.pkg.tar.zst`, `…-aarch64.pkg.tar.zst` | `sudo pacman -U muninn-bin-2.16.0-1-x86_64.pkg.tar.zst` |
| Homebrew | `muninn.rb` in `muninn-2.16.0-packaging.tar.gz` | `brew install ./packaging/homebrew/muninn.rb` |
| Windows (x64, arm64) | `muninn-2.16.0-x86_64-pc-windows-gnullvm.zip`, `…-aarch64-…zip` | unzip and put `mnnn.exe` on `PATH` (winget manifests are in the packaging tarball) |

Then try it somewhere disposable:

```sh
export MUNINN_HOME=/tmp/muninn-demo
mnnn keys init        # a password, then 24 words to keep offline
mnnn                  # the TUI; ? opens the help
```

Your data stays in `~/.muninn` (or `MUNINN_HOME`); installing or uninstalling never
touches it. Thanks for trying Muninn: tell us what you build with it on
[GitHub](https://github.com/WeirdBrains/muninn).
