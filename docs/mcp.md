# MCP

Muninn speaks the [Model Context Protocol](https://modelcontextprotocol.io) (MCP) both ways:

- **As a server**, so an AI assistant or another MCP client can read your journals,
  projects, models and records, reports, live searches, code-space results and org
  executables. If its role allows, it can also write and run things.
- **As a client**, so your rule sets can call the tools of external MCP servers and use
  them as data sources.

Nothing is served until you start `mnnn mcp serve`. External servers are only called by
you or by a rule set, and only after you approve each tool.

## Muninn as an MCP server

### Clients and roles

Every caller is a named MCP client with a role:

| Role | May |
|---|---|
| `read` (default) | list and read: the `list` and `read` tools and every resource |
| `write` | read, plus `write_entry`, `add_note`, `create_record`, `update_record`, `delete_record` |
| `run` | read, plus `run_search`, `run_code` (approved code spaces only), `run_executable` |
| `full` | everything above |

```
mnnn mcp client add claude                 # read only; prints its token once
mnnn mcp client add notebook --role write
mnnn mcp client list
mnnn mcp client role notebook full
mnnn mcp client revoke notebook            # refused from its next call on
mnnn mcp client restore notebook
mnnn mcp client remove notebook
```

A client's token is printed only when you add it. Muninn keeps only the token's hash. To
replace a lost token, remove the client and add it again.

### Serving

- **stdio**, for one client that starts Muninn itself, such as a desktop assistant:

  ```
  mnnn mcp serve --client claude
  ```

  A client configuration that starts it looks like this:

  ```json
  { "mcpServers": { "muninn": { "command": "mnnn", "args": ["mcp", "serve", "--client", "claude"] } } }
  ```

  Use `--home DIR` or `MUNINN_HOME` to serve a Muninn home other than the default. The
  database must open without prompting, so keep its key in the keyring as usual.
- **HTTP** (Streamable HTTP, with JSON responses), for clients that connect by URL:

  ```
  mnnn mcp serve --http                    # http://127.0.0.1:4743/mcp
  mnnn mcp serve --http 0.0.0.0:4743       # beyond this machine: see below
  ```

  Each request sends a client token as `Authorization: Bearer mnnn_mcp_…`. A missing,
  unknown or revoked token gets `401`. Requests from a browser page on another site
  (any `Origin` other than localhost) get `403`. Ctrl-C stops the server.

The HTTP server has no TLS of its own. It listens on 127.0.0.1 by default. If you bind it
beyond this machine, Muninn warns you; put it behind an HTTPS proxy and keep the tokens
secret.

### What it offers

**Tools:**

| Tool | Needs | Arguments |
|---|---|---|
| `list` | read | `what`: journals, entries, projects, models, records, reports, searches, org_resources, executables, views, history; `of`: the journal, model or org; `limit` |
| `read` | read | `uri`: a resource URI |
| `write_entry` | write | `journal`, `text`, `title` |
| `add_note` | write | `journal`, `text` |
| `create_record` | write | `model`, `values` |
| `update_record` | write | `model`, `id`, `values` |
| `delete_record` | write | `model`, `id` |
| `run_search` | run | `search`, `limit` |
| `run_code` | run | `space` (only once approved with `mnnn code approve`) |
| `run_executable` | run | `org`, `executable`, `input` (`where`, `text`, `sort`, `group`, `limit`) |

`tools/list` shows a client only the tools its role allows. Names and ids are accepted
wherever Muninn's CLI accepts them.

**Resources** (`resources/list`, `resources/read`; JSON):

| URI | What |
|---|---|
| `muninn://journal/ID` | a journal and its entries |
| `muninn://project/ID` | a project and its items |
| `muninn://model/ID` | a model's design |
| `muninn://model/ID/records` | its records (first 500) |
| `muninn://report/ID` | a report and its sections |
| `muninn://search/ID` | a live search |
| `muninn://view/ID` | a code space's last result |
| `muninn://org/ID/resources` | what an org holds |
| `muninn://org/ID/executables` | what this node serves for an org |

### What is recorded

- Every write and run goes into the history with origin `mcp` and the client's name, for
  example `write entry Day 1 in Lab (MCP client notebook)`. You can see it, undo it and
  react to it like any other action.
- A refused call is recorded as `mcp.denied`.
- Starting the server is recorded as `mcp.serve`.
- Reads are not recorded. Each client's "last seen" time is updated on every call.

## External MCP servers

### Adding one

```
mnnn mcp add weather --url https://weather.example/mcp --token -     # asks for the token, hidden
mnnn mcp add files --env API_KEY -- npx -y @example/files-mcp       # asks for API_KEY, hidden
mnnn mcp add local --env API_KEY=sk-… -- ./my-server --flag
mnnn mcp list
mnnn mcp show weather
mnnn mcp edit weather --header X-Team=research      # add or replace a credential
mnnn mcp edit weather --clear --token -             # forget them all, then set one
mnnn mcp remove weather
```

How credentials are passed:

- A **stdio** server is started for each use with its `--env` variables. Its standard
  error is discarded.
- An **HTTP** server is sent its `--header` values with every request. `--token T` is
  short for `--header "Authorization=Bearer T"`.

Credentials are kept in the encrypted database and are never printed. `show`, `list` and
`--json` name them without their values, and the history records only their names.
Leave out a value (`--env KEY`, `--header NAME`, `--token -`) and Muninn asks for it with
hidden input, so the value stays out of your shell history.

External servers, their credentials and approvals stay on the node where you added them.
They are not synced to your linked nodes.

### Tools and approval

```
mnnn mcp tools weather                    # what it offers (✓ approved)
mnnn mcp call weather forecast city=Oslo days=3
mnnn mcp approve weather forecast
mnnn mcp unapprove weather forecast
```

A tool must be approved before its first call:

- `mnnn mcp call` asks you the first time. `-y` answers yes.
- A rule set never approves on its own. The first time one wants an unapproved tool, it
  fails and you get a notification saying how to allow it.

Every call is recorded as `mcp.call`, with the server, the tool, the argument names (not
their values) and whether it worked.

Argument values that parse as JSON (numbers, `true`, lists, objects) are sent as JSON. Any
other value is sent as a string.

### In rule sets

- **As an action**, use `mcp SERVER TOOL key=value …`. The values are templates:

  ```
  mnnn rule create "Tell the team" --on import --if "inserted > 0" \
    --then 'mcp chat post channel=markets text="imported {inserted} bars of {params.symbol}"'
  ```

- **As a data source in conditions**, use a field of the form
  `mcp:SERVER/TOOL(key=value,…).path`. Muninn calls the tool when the rule could run and
  reads the result:

  - `path` is looked up in the tool's structured result, or in its text parsed as JSON.
  - With no path, the field is the whole result.
  - The arguments in parentheses are optional.

  ```
  mnnn rule create "Only when it's windy" --on import \
    --if 'mcp:weather/current(city=Oslo).wind_ms > 10' --then 'notify "windy import"'
  ```

  An unreachable server or an unapproved tool leaves the field empty. In that case `?`
  is false and `!?` is true.

- **In templates**, `{mcp:SERVER/TOOL.path}` works in texts too.

`mnnn rule dry-run` says whether a tool is approved. `mnnn rule events` lists the `mcp.*`
actions you can react to.

## In the TUI

Net ▸ MCP… (`M`) has two tabs:

- **Servers**: your external servers. `Enter` lists a server's tools (asking it now), `a`
  approves or unapproves the selected tool, `d` removes the server.
- **Clients**: the MCP clients. `r` revokes or restores one, `o` changes its role, `d`
  removes it.

Add servers and clients with the commands above; the TUI never shows tokens or credential
values.

## Failure modes

- **`401` or "this MCP client was revoked or removed"**: the token is wrong, or the client
  was revoked. Use `mnnn mcp client list` to check.
- **"`X` needs write" (or run)**: the client's role doesn't allow the tool. Change it with
  `mnnn mcp client role NAME write`.
- **"not approved yet"**: run `mnnn mcp approve SERVER TOOL`.
- **"could not start" or "no answer to … (it stopped or took over 30s)"**: a stdio
  server's command is wrong or it crashed. Its standard error is discarded, so run the
  command yourself to see why.
- **"refused (401 Unauthorized)"** from an external server: check its credentials with
  `mnnn mcp show NAME`, then fix them with `mnnn mcp edit NAME --token -`.

## Design notes

- The MCP protocol revision is 2025-06-18; clients asking for 2025-03-26 or 2024-11-05 are
  answered in that revision.
- The HTTP server answers every request with JSON. It keeps no sessions and offers no
  server-to-client stream (`GET` gets `405`).
- Muninn makes no prompts and no subscriptions yet.
