# weirdnet-rules

If-this-then-that over an event history, without knowing what the events or the actions are.

- `RuleSpec<T>`: `on` event-kind patterns (`import`, `journal.*`, `*`), `if` conditions,
  `then` actions of the application's own type `T`, `related`, and an optional schedule.
  Serialized as versioned JSON.
- `syntax`: conditions (`inserted > 0`, `title ~ "btc halving"`, `out ?`), lists, words,
  quoting, and `on` patterns checked against the application's event catalog.
- `compare`: how a condition's operator compares a JSON value (numbers, text, globs,
  arrays, missing values).
- `schedule`: `every 6h` and five-field cron (local time or any `chrono` zone), next run.
- `template`: `{field}`, `{rule}`, `{date}`, `{time}`, `{now}` filled from a lookup.
- `engine`: matching (`check`), loop protection (a rule never re-enters itself, at most
  `MAX_DEPTH` rules in a chain and `MAX_RUNS` runs per outermost event), running a rule's
  then-actions in order and collecting their results, and which scheduled rules are due.

The engine reaches the application only through `Host`: it lists the enabled rules, reads
event fields, says whether an event is about where a rule lives, records the start and end
of a run (and blocked runs), and executes one then-action. What the actions do, who may
create or run rules, where rules are stored, and when the scheduler ticks are the
application's. The crate does no I/O and decides no permissions.
