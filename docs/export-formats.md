# Export formats

Journals, entries, reports and projects are exported through one document layer, and
bars can be exported as documents too ([export](export.md)).

| format | what you get | journals | reports | projects |
|--------|--------------|:-:|:-:|:-:|
| PDF | for reading and printing (A4) | ✓ | ✓ | ✓ |
| HTML | one self-contained page, images inline | ✓ | ✓ | ✓ |
| Word (.docx) | for Word, LibreOffice, Google Docs | ✓ | ✓ | ✓ |
| LaTeX (.tex) | the source PDF and Docx are made from; images written beside it | ✓ | ✓ | ✓ |
| JSON | the data: importable journals and projects, a report's definition and tables | ✓ | ✓ | ✓ |
| CSV | the document's tables, with a leading `section` column | | ✓ | ✓ |
| text | plain text, as `report preview` shows it | ✓ | ✓ | ✓ |
| archive (.mnnp) | the whole project, see below | | | ✓ |

```
mnnn journal export JOURNAL [--entry ENTRY]... [--format F] [--out PATH]   # default pdf
mnnn report export REPORT [--format F] [--out PATH]                        # default pdf
mnnn project export PROJECT [--format archive|json|pdf|…] [--out PATH] [--rows N] [--no-data]
```

In the TUI, `E` opens the export window in [journals](journals.md) (the journal, the
marked entries or the open entry), [reports](reports.md) and [projects](projects.md):
choose a format, then confirm or change the path (`<current dir>/<name>.<ext>`).
Exports are recorded in the [history](history.md) (`journal.export`, `entry.export`,
`report.export`, `project.export`).

## How documents are made

Everything is generated inside `mnnn`; no TeX, LibreOffice, pandoc or other program is
needed at run time, and packages stay prebuilt binaries.

1. The exporter builds a document (headings, paragraphs, lists, tables, code, math,
   images, notes, highlights) from the store. Journal Markdown is parsed; journal
   LaTeX is read by the same reader as step 3.
2. **LaTeX is the source.** The document is written as LaTeX (`article`, `graphicx`,
   `longtable`, `amsmath`, `hyperref`, `xcolor` and `soul` for highlights; notes are
   numbered marks with a list at the end of their section). The `.tex` export is this
   file and compiles with a normal TeX installation.
3. **PDF and Word are generated from that LaTeX**: Muninn reads its own LaTeX back
   (plus the commands journal entries commonly use) and lays it out itself. The PDF
   writer uses bundled Liberation Serif and Mono fonts (SIL Open Font License, shipped
   as `LICENSE-FONTS`), subset into each file; Word documents are written as Office Open
   XML directly.
4. HTML and text are written from the same document; JSON and CSV from the data.

Why not a TeX engine? Embedding one (Tectonic) pulls in C/C++ libraries (harfbuzz,
ICU, freetype, graphite) that are hard to build for every release target, and it
downloads TeX packages on first use. Typst is pure Rust but is not LaTeX and adds a
large dependency tree. The pure-Rust writers (`pdf-writer`, `ttf-parser`, `subsetter`,
`png`, `flate2`) build on every CI target: Linux, macOS and Windows on x86_64 and
aarch64, and the deb, rpm and Arch packages.

Limits of the built-in layout:

- Math is shown as text with common commands turned into Unicode (`\alpha` → α,
  `x^2` → x²); there is no TeX math layout. Compile the `.tex` export for that.
- No kerning, ligatures or complex-script shaping; text outside the fonts' Latin,
  Greek and Cyrillic coverage shows as `?` in PDF (HTML, Word and text keep it).
- Notes are numbered and listed at the end of their section (endnotes), not at the
  foot of the page.
- PDF embeds PNG and JPEG images, Word PNG, JPEG, GIF and BMP; other images (WebP,
  SVG, …) are kept in HTML and LaTeX and shown as a named placeholder.

## Printing

```
mnnn journal print JOURNAL [--entry ENTRY]... [--printer NAME] [--out PATH] [--save-only]
```

In the TUI, `P` prints the journal (or the marked entries) or the open entry. Muninn
makes a PDF (by default in `<home>/prints/`), then hands it to `lp` (or `lpr`) when one
is installed; otherwise, or with `--save-only`, it keeps the PDF and says where it is.
A printer error is reported with the PDF's path. Recorded as `journal.print`.

## Project archives and import

```
mnnn project export PROJECT [--out NAME.mnnp] [--no-data] [--rows N]
mnnn project import FILE [--name NAME]
mnnn journal export JOURNAL --format json --out FILE
mnnn journal import FILE [--project PROJECT] [--name NAME]
```

A project archive (`.mnnp`, a zstd-compressed tar) holds `muninn-project.json` (the
project, its live searches, results, journals with entries, notes and highlights,
reports with their sections, and the authors involved), the images under
`attachments/`, the bars the project refers to under `data/` as CSV (result ranges,
what its live searches match and the series its report sections show; `--no-data`
leaves them out), and `project.html` to read without Muninn. `--format json` writes
the same project as one JSON file with images and bars inline.

Import restores everything as new items with new ids, so it can sit next to the
original: a project or journal of the same name is imported as "NAME (imported)"
unless `--name` is given, and so are a project's journals and reports whose names are
taken. Bars are added like an ingest (existing bars are kept),
authors are matched by id and added when missing, and references between the
project's journals, live searches, reports and images are rewritten. An import is all
or nothing. Recorded as `project.import` / `journal.import`.

See also: [reports](reports.md), [projects](projects.md), [journals](journals.md),
[export](export.md).
