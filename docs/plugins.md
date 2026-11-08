# Plugins

A **plugin** is a [code space](code-spaces.md) seen by its **kind**. The space carries the
kind (code has none, so one code can serve spaces of different kinds); the "manifest" is
the space row: kind, main, members.

| kind | does | where it runs |
|------|------|---------------|
| `parser` | raw bytes → rows | a pipeline/pipe step (`space:NAME`) |
| `transform` | frame → frame | a pipeline/pipe step |
| `analysis` | runs for a view | on its own, over a share, a rule's `run-space` |
| `ingest-connector` | fetches, stores raw, pipes, runs a rule set | a schedule, or a [dry run](connectors.md#dry-runs) |

Rules:
- A space's kind is required and is one of the four (`mnnn spaces kind NAME K` changes it;
  the space then needs approving again).
- Only an ingest connector's **main** gets the connector host methods (`context`,
  `store_raw`, `pipe`, `run_ruleset`, `fetch`, `state_*`); other kinds never do.
- A main can `call_space` parser, transform and analysis spaces, never a connector.
- A **prebuilt** library (SDK 1.3 `plugin!(run, kind = "…")`) that declares a kind must
  be main only of spaces of that kind: a mismatch stops the run with the fix. The library
  is read only once the space is approved, because loading it runs its code. A library
  declaring no kind (built before SDK 1.3) runs as its space's kind.
- There is no `mnnn plugins migrate`: [`mnnn migrate`](code-spaces.md#migrating) assigns
  kinds when it turns code into spaces.

## `mnnn plugins`

```
mnnn plugins list [--kind K] [--json]          # name, kind, main's language, approved, enabled
mnnn plugins info NAME [--json]                # kind, main, library and its declared kind, used by
mnnn plugins install PATH|CODE --kind K [--name N]   # a library or a code, as a one-member space
mnnn plugins enable|disable NAME               # disabled plugins don't run
mnnn plugins remove NAME                       # the space (its code stays); asks, -y skips
```

`install` with a path makes a `plugin` code pointing at the library, plus the space; with
a code name it makes the space around that code. Either lands **unapproved**: approve it
with `mnnn spaces approve NAME`. "Used by" lists pipelines that run the space, rules that
`run-space` it and other mains that import it.

## The Plug menu

**Plug** (Alt+L): **Parsers… · Transforms… · Analysis… · Ingest connectors… · All
plugins…**. Each opens the code spaces window filtered to that kind, with its keys:
Enter members, `r` run, `a` approve, Space enable/disable, `p` add to project, `g` give
to an org, `S` share. An empty kind shows how to install one.
