# Reviews

A review asks peers to look at one thing of yours: an [entry](journals.md), a
[journal](journals.md) and its entries, a [project](projects.md), a
[live search](live-searches.md) or a [model](models.md) record. Reviewers can view it,
comment, suggest changes, highlight text and vote. You accept or reject the suggestions
and merge the accepted ones, which makes a new numbered revision of the thing.

## Asking for a review

```
mnnn review request journal "field notes" --note "before Friday" --allow comment,suggest
mnnn review request record trades/3f2a --label ben --peer /ip4/…/p2p/…
mnnn review invite "field notes" --label cleo
```

`request` takes a kind (`entry`, `journal`, `project`, `search` or `live-search`,
`record`; plurals work too) and a name or id; a record is `MODEL/RECORD`. It makes the
first link unless `--no-link`; `invite` makes more. With `--peer` the link is delivered
to that node, otherwise the token (`mnnn-x1-offer:…`) is printed to pass on.

- **A link is a [key exchange](key-exchanges.md)** scoped to the review
  (`{"kind":"resource","as":"review"}`): good for one reviewer, for 7 days. A used link
  is refused to anyone else.
- **Permissions.** `--allow` narrows what reviewers may do: `comment`, `suggest`,
  `highlight`, `vote`. `view` is always allowed; the default is everything.
- Reviewers reach the review through your node, so keep it running
  (`mnnn net start`, or `S` in the TUI's [network](network.md) window).
- Reports are not reviewable.

## Reviewing

```
mnnn review join mnnn-x1-offer:…        # or `-` to read it from stdin
mnnn review view "field notes"
mnnn review comment "field notes" "a fine start"
mnnn review suggest "field notes" --entry 3f2a --at "the cat" --to "the comet" --why "it is a comet"
mnnn review highlight "field notes" --entry 3f2a --at "hill" --color yellow --note "lovely"
mnnn review vote "field notes" approve        # approve, changes or reject
mnnn review vote "field notes" up --on 9c1e   # up or down on an item
mnnn review withdraw "field notes" 9c1e
mnnn review sync "field notes"
mnnn review leave "field notes"
```

Every command taking REVIEW works on your own reviews and on the ones you joined (those
go through the author's node). Anchors: `--field` (default: an entry's `body`, a
journal's `name`, a project's `description`, a search's `query`; records name a field),
`--at TEXT` with `--nth N` for the Nth occurrence, and `--entry` for an entry of a
journal review. A suggestion replaces the `--at` text, or the whole field, with `--to`
(`--json` for a record's typed value). Replies use `comment --reply ITEM`.
Each person has one vote on the review and one on each item; voting again changes it.

## Accepting and merging

```
mnnn review show "field notes"
mnnn review accept "field notes" 9c1e 4b2d     # or --all
mnnn review reject "field notes" --all
mnnn review merge "field notes"                # --force applies suggestions that no longer fit
mnnn review revisions journal "field notes"
```

Only suggestions are accepted or rejected. A merge applies the accepted ones and gives
each changed thing (the entry, journal, project, search or record) a new revision with
its before and after values and the items in it. A suggestion whose quoted text is gone,
or whose whole field changed since it was made, is a conflict: it stays accepted and is
reported, unless `--force`.

`close` ends the review: the reviewers' exchanges are revoked and they are told.
`reopen` opens it again; reviewers need new links. `drop REVIEW REVIEWER` takes one
reviewer off. `remove` deletes a review and its thread (it asks first); revisions stay.
For a review you joined, `remove` forgets it.

## The Reviews window (TUI)

`V`, or **Projects ▸ Reviews** (Reviews…, Ask for a review…, Thread…, Reviewing
(joined)…). Pages: `1` My reviews, `2` Thread, `3` Reviewing; Tab / ← → switch,
`j` / `k` move, `g` refresh, Esc closes.

| page | keys |
|------|------|
| My reviews | `n` ask for a review (`journal NAME`, `record MODEL/RECORD`, …), Enter its thread, `i` a link (optionally delivered to a peer: its node address or name), `p` view, `H` revisions, `o` close / reopen, `d` delete (`y` confirms) |
| Thread | `c` comment, `e` suggest, `h` highlight, `v` vote, `r` reply, `a` / `x` accept / reject the selected suggestion, `A` / `X` all, `m` merge, `F` merge anyway, `w` withdraw, `D` drop the last joined reviewer; `i`, `p`, `H` as above |
| Reviewing | `j` join a link, Enter view and thread, `s` sync, `c` / `e` / `h` / `v` add items, `l` leave (`y` confirms) |

Item prompts take optional `field=F entry=E nth=N` first, then `@quoted text :: the
rest` to anchor it: `entry=3f2a @the cat :: the comet` suggests a replacement. Without
`@` the whole field is meant. A vote is `approve`, `changes` or `reject`, or
`up ITEM` / `down ITEM`.

## History and rules

Every step is in the [history](history.md) under the current author: `review.request`,
`review.invite`, `review.join`, `review.joined`, `review.comment`, `review.suggest`,
`review.highlight`, `review.vote`, `review.withdraw`, `review.accept`, `review.reject`,
`review.merge`, `review.close`, `review.closed`, `review.reopen`, `review.drop`,
`review.leave`, `review.remove` and `review.serve`. A merge also records the edit itself
(`entry.edit`, `journal.edit`, `project.edit`, `search.edit` or `record.update`) with the
review's id. [Rules](rules.md) can react to all of them (`mnnn rule events`).
