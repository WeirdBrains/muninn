# Org site templates (2.17)

A template gives an org's [site](org-hosting.md) its look: an HTML page, a stylesheet and
files (images, fonts, scripts) served under `assets/`. Since 2.17 you can write and preview
templates in the app, build them from a framework (React, Jaspr, …), and other nodes hear
when your site opens.

| | CLI | TUI (Orgs ▸ Templates, `7`) |
|---|---|---|
| List | `mnnn org template list ORG` | the page |
| Show | `mnnn org template show ORG T [--json]` | the page shows the selected one's files and build step |
| New / edit HTML | `mnnn org template edit ORG T` (opens `$VISUAL` / `$EDITOR`; a new name starts from the built-in look) | `n` new, `e` / Enter edit |
| Edit CSS | `mnnn org template edit ORG T --css` | `c` |
| Save from files | `mnnn org template save ORG T --html FILE [--css FILE] [--use]` | |
| Files | `mnnn org template asset ORG T FILE [--name N]`, `… asset ORG T NAME --remove` | `a` add (`PATH [NAME]`), `x` remove |
| Use it for the site | `mnnn org edit ORG --template T` (or `save --use`) | `u` (again: back to the built-in look) |
| Preview | `mnnn org template preview ORG T` | `p`, or Ctrl-P while editing |
| Build step | `mnnn org template build ORG T --cmd CMD [--src DIR] [--out DIR] [--no-run]`, `--clear` | `b` (`FOLDER \| COMMAND [\| OUTPUT]`, or `clear`), `B` runs it |
| Remove | `mnnn org template remove ORG T` | `d` |

All of it needs `publish` in the org. Every change is in the [history](history.md)
(`org.template.save`, `.asset`, `.asset_remove`, `.build_set`, `.build`, `.preview`, `.remove`).

## Placeholders

A template fills `{{title}}`, `{{org.name}}`, `{{org.description}}`, `{{org.slug}}`,
`{{base}}` (the site's address), `{{assets}}` (where its files are, ending in `/`),
`{{css}}` (the stylesheet, to put in a `<style>`) and `{{content}}` (the page, already
HTML). Everything but the content is escaped.

## Editing in the TUI

`e` (HTML) or `c` (CSS) opens the template in the window's editor: arrows, Home/End,
PgUp/PgDn, Tab indents. **Ctrl-S** saves, **Ctrl-P** makes a preview link, **Esc** leaves
(with unsaved changes, Esc once warns and Esc again leaves without saving).

## Previews

A preview shows the org's page through a template, whether or not the org uses it or is
hosted yet, at `/o/SLUG/_preview/TOKEN/` on the local site (with its files under
`…/TOKEN/assets/`). The page is rendered again on every load, so save and reload to see an
edit. A preview:

- lasts **an hour**; make a new one after that;
- shows **public artifacts only**, whatever you could see yourself;
- is reached by its token alone: anyone with the link and access to the site can open it
  while it lasts, so share it as you would a draft.

If the site is not running, the command prints the path; `mnnn org host start` (or `S` on
the Site page) serves it.

## Build steps

A compiled template (React, Jaspr, Svelte, …) can be built by Muninn:

```sh
mnnn org template build comet-lab site --cmd "npm ci && npm run build" --src ~/sites/comet --out dist
```

- The source folder is **copied to a scratch folder** (without `.git`) and the command runs
  there with `sh -c` (`cmd /C` on Windows); your folder is never changed.
- The output folder (default `build`) must have an `index.html`; it becomes the template's
  HTML. A `style.css` becomes its CSS; every other file becomes one of its files, by its
  path (`img/logo.png`), and the old files are replaced. The output may be up to 64 MB.
- The command's output is shown (its last 40 lines); a failing command changes nothing.
- An `index.html` without `{{content}}` builds, with a warning: such a page shows only the
  template.

**Consent.** A build runs only after the person at this node approved that exact command,
source folder and output folder for that template: the CLI asks (`--yes` answers), the TUI
asks `y/n`. Any change to the three asks again. Approvals are kept on this node only and
never sync, so a build step set on another of your nodes, or by another member, never runs
here without being approved here. The build step itself (command and folders) syncs with
the template; the source folder must exist on the node that builds.

## Announcing the site

When the site starts (`mnnn org host start`, the node's autostart, or `S` in the TUI),
Muninn tells others where it is and what it serves; when it stops, that it closed.
`mnnn org announce [ORG] [--closed]` (or `A` on the Site page) announces again.

- **Who hears it:** the org's members' nodes (their membership exchanges with this node),
  your own linked nodes, and, **for public orgs only**, your connections.
- **What they hear:** the org's name, slug, description, visibility, whether the site is
  gated, its addresses and its public artifacts (`journal: Field notes`), open or closed.
- **Addresses:** the org's domains, then `https://HOST:PORT/o/SLUG/` for each external
  address (`mnnn net external`), each `[hosting] names` entry and a non-loopback bind
  address. A site bound to `127.0.0.1` with none of these is only reachable from this
  machine, so it is not announced (you are told why).
- Announcing is best effort: unreachable nodes are listed and skipped.

What others announced to this node: `mnnn org sites [--all] [--json]` and the Site page
(open ones; `--all` adds closed ones). A node accepts an announcement only over an exchange
that may carry it: a membership of that org, a node link, or a connection for a public org.
Text is clipped and only `https://` addresses are kept.

## Decisions

Choices made for 2.17 where the plan left room:

- Build approvals are a hash of command + source + output folder, per template and per
  node; nothing approves builds for other nodes.
- Builds copy the source to a scratch folder instead of running in place, so a build can't
  change your working tree, and the output replaces the template's files wholesale.
- Previews use an unguessable, expiring token on the local site rather than a member
  sign-in, so they work before the org is hosted; they show public artifacts only.
- Announcements reuse existing key exchanges (no new discovery) and go to connections only
  for public orgs, so a private org's existence is never told to people outside it.
- The TUI edits templates in its own editor; the CLI uses `$VISUAL` / `$EDITOR`.
