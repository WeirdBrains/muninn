# Muninn 2.0

![Three pipeline views as three live columns: price, volume and the tape](../site/web/screenshots/pipeline-live.png)

Muninn (`mnnn`) is a local, encrypted workbench for market data. It ingests bars and
records from files and services, keeps them in a SQLCipher database on your machine, and
gives you a terminal UI and a CLI to search, journal, model and share them. Muninn 2.0
is the result of twenty-two phases of work, released as v2.0.0 through v2.18.0. Here is
what it brings. The core from 1.x (ingest, encryption, backups, export, charts, tiling and
themes) is in the [Muninn 1.0 announcement](https://github.com/WeirdBrains/muninn/blob/main/docs/historical/1.0/ANNOUNCEMENT.md).

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
- **Organizations**: roles, members and hosted HTTPS sites (HTML, JSON and gRPC), with your own domains and Let's Encrypt certificates that renew themselves. Private orgs can gate their site: guests see only a landing, sign-in and join page.

![Peers: nodes seen nearby and through relays](../site/web/screenshots/peers.png)

![Reviews: comments, suggestions, highlights and votes](../site/web/screenshots/reviews.png)

- **Org site templates in the app**: edit a site's HTML and CSS in the TUI (Orgs ▸ Templates) or in `$EDITOR`, manage its files, preview it live through the local site, and add build steps for React, Jaspr and other compiled templates. Build steps run in a scratch copy, and each node asks before running a new or changed command. A starting or stopping site tells the org's nodes, your linked nodes and connections where it is.

![Org site templates: HTML and CSS, files, previews and build steps](../site/web/screenshots/org-templates.png)

## Notifications and broadcasts

- **Notifications**: exchange requests, org applications, review comments and votes, sync conflicts, code waiting for approval and certificate warnings all notify you. A bell in the TUI header shows the unread count. `!` opens the list, and `Enter` takes you to the review, org, journal or project a notification is about. From the shell, use `mnnn notify list`. You can mute any kind.
- **Rule sets notify, and react to notifications**: `then notify "imported {inserted} bars" level=warn`, or `--on notification --if "level = error"`.
- **Node broadcasts**: `mnnn net broadcast deploy "v2 is out" --to connections` (or your linked nodes, or an org's nodes) sends a short message over the same trusted key exchanges as everything else. It arrives as a notification and as a trigger that rule sets can act on, and rule sets can send broadcasts too.

![Notifications: the header bell and the window](../site/web/screenshots/notifications.png)

- **Clickable hints**: every key shown in a hint is also a button you can click.

## Install

Every package ships a prebuilt `mnnn` binary; no Rust is needed. Downloads and
`SHA256SUMS` are on the [v2.18.0 release](https://github.com/WeirdBrains/muninn/releases/tag/v2.18.0)
(full steps in [install](install.md)).

| Platform | Download | Install |
|----------|----------|---------|
| Linux or macOS (tarball) | `muninn-2.18.0-<target>.tar.gz` for `x86_64`/`aarch64-unknown-linux-gnu` and `x86_64`/`aarch64-apple-darwin` | `tar -xzf …` then `install -Dm755 muninn-2.18.0-<target>/mnnn ~/.local/bin/mnnn` |
| Debian / Ubuntu | `muninn_2.18.0-1_amd64.deb`, `muninn_2.18.0-1_arm64.deb` | `sudo apt install ./muninn_2.18.0-1_amd64.deb` |
| Fedora | `muninn-2.18.0-1.x86_64.rpm`, `muninn-2.18.0-1.aarch64.rpm` | `sudo dnf install ./muninn-2.18.0-1.x86_64.rpm` |
| Arch Linux | `muninn-bin-2.18.0-1-x86_64.pkg.tar.zst`, `…-aarch64.pkg.tar.zst` | `sudo pacman -U muninn-bin-2.18.0-1-x86_64.pkg.tar.zst` |
| Homebrew | `muninn.rb` in `muninn-2.18.0-packaging.tar.gz` | `tar -xzf muninn-2.18.0-packaging.tar.gz`, then `brew install ./packaging/homebrew/muninn.rb` |
| Windows (x64, arm64) | `muninn-2.18.0-x86_64-pc-windows-gnullvm.zip`, `…-aarch64-…zip` | unzip and put `mnnn.exe` on `PATH`, or `winget install --manifest packaging\winget\2.18.0` from the packaging tarball |

Then try it somewhere disposable:

```sh
export MUNINN_HOME=/tmp/muninn-demo
mnnn keys init        # a password, then 24 words to keep offline
mnnn                  # the TUI; ? opens the help
```

Your data stays in `~/.muninn` (or `MUNINN_HOME`); installing or uninstalling never
touches it. Thanks for trying Muninn: tell us what you build with it on
[GitHub](https://github.com/WeirdBrains/muninn).
