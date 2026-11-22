# Handoff

Running notes for whoever picks up Muninn next. Dated entries, newest first.

## 2026-11-22: public content clean-up

- Branch `public-content-cleanup`, merged into main with `--no-ff`. No release or tag:
  that waits for m3talsmith's OK.
- Applies the public content rules (Weird Brains LLC, m3talsmith, no host names, home
  paths or addresses, weirdbrains.com links) across code comments, tests, docs, the
  site, the plans and both historical folders.
- Behaviour change: the TUI import `raw/data` place now defaults to `~/data/raw` when
  `MUNINN_RAW_DATA` is unset. Set `MUNINN_RAW_DATA` to keep using another folder.
- `tests/fixtures/bars-2.14` was regenerated with `scripts/make-bars-2.14-fixture.sh` and
  the released mnnn 2.14.0 binary, run from `/home/user/muninn`, so the encrypted
  fixture database records only generic paths. Ids, keys and timestamps changed with it.
- Not changed: commit authorship in the existing history.
