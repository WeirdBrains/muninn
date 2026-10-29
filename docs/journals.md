# Journals

A journal is a list of entries: notes on what you see in the data, trade logs, ideas.
A journal is **standalone** or belongs to a [project](projects.md) (a *project
journal*); any journal can also be linked into other projects.

- **Entries** have a title and a text in **Markdown** or **LaTeX**. A journal has a
  default format for new entries; each entry keeps its own.
- **Images** are stored in the encrypted database and referenced from the text:
  `![name](attachment:ID)` in Markdown, `\includegraphics{attachment:ID}` in LaTeX.
- **Notes** go on the journal, on an entry, or on selected words of an entry.
- **Highlights** mark selected words of an entry (with a colour and an optional
  comment).
- **Fork** a journal into a project: a copy of everything in it (entries, notes,
  highlights, images) that belongs to that project.
- **Copy** selected entries into another journal or a new one; their notes,
  highlights and images come along.

Everything is attributed to the current [author](authors.md) and recorded in the
[history](history.md).

Selections are character ranges of the entry text, with the words they cover saved
alongside. When an entry is edited, notes and highlights follow their words; when those
words are gone they stay on the entry, marked *detached*.

## Command line

```
mnnn journal list [--project PROJECT] [--json]
mnnn journal new NAME [--project PROJECT] [--format markdown|latex]     # alias: create
mnnn journal show JOURNAL [--json]            # --json: the whole document
mnnn journal edit JOURNAL [--name NAME] [--format markdown|latex]
mnnn journal remove JOURNAL                   # asks first; -y skips
mnnn journal fork JOURNAL [--project PROJECT] [--name NAME]

mnnn journal write JOURNAL [--title TITLE] [--format F] (--text TEXT | --file PATH|-)
mnnn journal entry ENTRY [--json]
mnnn journal edit-entry ENTRY [--title T] [--format F] [--text TEXT | --file PATH|-]
mnnn journal remove-entry ENTRY
mnnn journal copy ENTRY... (--to JOURNAL | --new NAME [--project PROJECT])

mnnn journal note BODY (--journal JOURNAL | --entry ENTRY [--on WORDS [--nth N] | --range A..B])
mnnn journal highlight ENTRY (--on WORDS [--nth N] | --range A..B) [--color C] [--comment TEXT]
mnnn journal edit-note ID BODY
mnnn journal remove-note ID                   # notes and highlights (alias: unmark)

mnnn journal attach ENTRY FILE [--name NAME]  # adds the reference to the entry text
mnnn journal image ID [--out PATH]            # writes the image back out
```

Journals, entries, notes and images are named by id or unique id prefix; journals also
by name. `--text` turns `\n` into a new line; `--file -` reads standard input.
`--on WORDS` selects the first occurrence of the words (or the `--nth`); `--range`
takes character offsets, counted from 0, end exclusive.

```
mnnn journal new "Summer notes" --project "ETH summer"
mnnn journal write "Summer notes" --title "June 3" --text "Breakout above 3800 on volume"
mnnn journal highlight 1a2b3c4d --on "above 3800" --comment "resistance"
mnnn journal note "check the 1h chart" --entry 1a2b3c4d --on breakout
mnnn journal attach 1a2b3c4d ~/charts/june3.png
mnnn journal copy 1a2b3c4d 5e6f7a8b --new "Best setups"
```

## Terminal UI

**Projects ▸ Journals…** or `J` lists the journals (with their project);
**Projects ▸ New journal…** creates one.

| key (list) | action |
|-----|--------|
| Enter | open the journal |
| `n` | new journal |
| `r` | rename |
| `t` | switch the default format (Markdown ↔ LaTeX) |
| `N` | note on the journal |
| `f` | fork into a project (or standalone) |
| `p` | add to a project |
| `D` | delete the journal (`y` confirms) |

| key (journal) | action |
|-----|--------|
| `j` / `k` | move |
| Enter | read the entry |
| `w` / `n` | write a new entry |
| `e` | edit the entry |
| Space | mark / unmark entries |
| `c` | copy the marked entries (or the selected one) to another journal or **+ New journal…** (a new journal joins this journal's project) |
| `d` | delete the entry (`y` confirms) |
| `N`, `f`, `p`, `r`, `t`, `D` | as in the list |
| Esc | back to the list |

Reading an entry shows its text, with highlights and noted words marked, then its
notes, highlights and images. The cursor is a word:

| key (entry) | action |
|-----|--------|
| ← → / `h` `l` | previous / next word |
| `w` / `b`, Home / End | five words forward / back, first / last word |
| `v` | start (or drop) a selection at the cursor; move to extend it |
| `m` | highlight the selection (or the word under the cursor) |
| `n` | note on the selection, or on the entry when nothing is selected |
| `N` | note on the journal |
| `j` / `k` | select a note or highlight |
| Enter | edit the selected note (or the highlight's comment) |
| `x` | delete the selected note or highlight (`y` confirms) |
| `i` | attach an image (type its path; `~/` works) |
| `e` / `d` | edit / delete the entry |
| PgUp / PgDn | scroll |
| Esc | drop the selection, then back to the journal |

The entry editor has a title and a text field: Tab switches, Enter in the title moves to
the text, **Ctrl-S** saves, **Ctrl-T** switches Markdown ↔ LaTeX, Esc cancels.

A [live search](live-searches.md) on journals (`--journals`) lists the matching
entries; Enter reads one.

## Later

Export, import and print (HTML, JSON, LaTeX, PDF, Docx) come with Phase 3. `mnnn
journal show --json` already writes the whole journal as one document (entries,
notes, highlights and image metadata), which is what the exporters will read.

See also: [projects](projects.md), [live searches](live-searches.md),
[authors](authors.md), [history](history.md).
