# Authors

An author is a person Muninn attributes work to: a name, an optional email and any
extra details (`key=value`, e.g. `org=Acme`, `role=analyst`). Authors live in the
encrypted database. One of them can be the **current author**: every action in the
[history](history.md) is stamped with the author who was current when it ran, and later
features (reports, reviews, organizations) will use it as well.

The first author you add becomes the current one.

## Command line

```
mnnn author list [--json]                       # * marks the current author
mnnn author show [AUTHOR] [--json]              # default: the current author
mnnn author add --name NAME [--email EMAIL] [--detail KEY=VALUE]... [--current]
mnnn author edit AUTHOR [--name NAME] [--email EMAIL] [--detail KEY=VALUE]... [--replace-details]
mnnn author remove AUTHOR                        # asks first; -y skips the question
mnnn author use AUTHOR | --none                  # set or clear the current author
```

`AUTHOR` is an id, a unique id prefix (the 8 characters `list` shows), a name or an
email (names and emails ignore case). In `edit`, `--email ""` clears the email,
`--detail key=` removes a detail, and `--replace-details` replaces all details with the
ones given. `mnnn authors` is an alias of `mnnn author`.

```
mnnn author add --name "Ada Lovelace" --email ada@example.org --detail org=Analytical
mnnn author use ada@example.org
mnnn author edit Ada --detail org= --detail team=engines
```

## Terminal UI

**File ▸ Authors…** or `A` opens the authors popup. The current author is marked `★`
and is also shown in the header (`author: NAME`).

| key | action |
|-----|--------|
| `j` / `k`, ↑ / ↓ | move |
| Enter, Space, click a selected row | make it the current author |
| `a` | add an author (form: Name, Email, Details) |
| `e` | edit the selected author |
| `d` | delete the selected author (`y` confirms) |
| `x` | clear the current author |
| Esc, [X] | close |

In the form, Tab / ↓ and ↑ move between fields, Enter saves and Esc cancels. Details
are written `key=value; key=value`.

## Notes

- Removing an author keeps its id on past actions; `mnnn history show` then prints
  `(removed author)`.
- Adding, editing, removing and choosing the current author are themselves recorded in
  the [history](history.md).

See also: [history](history.md), [data model](data-model.md).
