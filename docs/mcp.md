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
  mnnn mcp serve --tls-domain mcp.example.com --tls-email you@example.com
                                           # https://mcp.example.com/mcp: see below
  ```

  Each request sends a client token as `Authorization: Bearer mnnn_mcp_…`. A missing,
  unknown or revoked token gets `401`. Requests from a browser page on another site
  (any `Origin` other than localhost) get `403`. Ctrl-C stops the server.

Plain HTTP listens on 127.0.0.1 by default. If you bind it beyond this machine without TLS,
Muninn warns you. To serve other machines, use HTTPS: built in, or through a proxy.

### HTTPS with Let's Encrypt

Give the server a public name and it gets its own certificate from Let's Encrypt, serves
HTTPS on every address at port 443, and renews the certificate while it runs:

```
mnnn mcp serve --tls-domain mcp.example.com --tls-email you@example.com
# https://mcp.example.com/mcp
```

1. **DNS**: an `A` (and `AAAA`, if the machine has IPv6) record for the name, pointing at
   the machine. Check with `dig +short mcp.example.com`.
2. **Firewall**: open TCP 80 (the HTTP-01 challenge; Muninn answers only
   `/.well-known/acme-challenge/` there) and 443. Keep 4743 closed.
   `sudo ufw allow 80/tcp && sudo ufw allow 443/tcp` (or `firewall-cmd --add-service=http
   --add-service=https --permanent && firewall-cmd --reload`).
3. **Ports below 1024 without root**: run it as a systemd service with
   `AmbientCapabilities=CAP_NET_BIND_SERVICE` (below), or give the binary the capability
   once: `sudo setcap 'cap_net_bind_service=+ep' "$(command -v mnnn)"` (again after every
   upgrade, since a new binary loses it). Or pick other ports: `--http 0.0.0.0:8443` with
   `--acme-challenge tls-alpn-01` validates through the HTTPS port itself, so it needs only
   443 forwarded to 8443; `--acme-http-port 8080` needs 80 forwarded to 8080.
4. **Try it on staging first**: `--acme-staging` uses Let's Encrypt's staging CA, whose
   certificates browsers don't trust but whose rate limits are generous. Drop it once that
   works (the staging certificate is replaced on the next start, since it is for a
   different CA).

Until the first certificate arrives (a few seconds), the server answers with a self-signed
one. The certificate is kept in the database (setting `mcp.acme.cert`, apart from the
[org site's](org-hosting.md)); the ACME account is shared with org hosting. It is
Let's Encrypt's short-lived profile (about six days), checked every minute and renewed
when a third of its life is left; failures are retried with back-off and logged. A
restart serves the kept certificate at once.

The same settings can live in `preferences.toml`, so `mnnn mcp serve --http` alone serves
HTTPS (flags override them):

```toml
[mcp]
bind = ""                 # default: 127.0.0.1:4743, or 0.0.0.0:443 with TLS

[mcp.acme]
enabled = true
domain = "mcp.example.com"
email = "you@example.com" # default: [hosting.acme] email
directory = "letsencrypt" # letsencrypt-staging, or another ACME directory URL
challenge = "http-01"     # or tls-alpn-01
http_port = 80
```

**A certificate of your own** instead: `--tls-cert fullchain.pem --tls-key privkey.pem`
(or `[mcp] tls_cert`, `tls_key`). It is not renewed; restart the server after replacing
the files.

With TLS on, a browser page on the server's own name may call it; other origins still
get `403`.

#### As a service

```ini
# /etc/systemd/system/muninn-mcp.service
[Unit]
Description=Muninn MCP server
After=network-online.target
Wants=network-online.target

[Service]
User=muninn
Environment=MUNINN_HOME=/var/lib/muninn
Environment=MUNINN_NON_INTERACTIVE=true
EnvironmentFile=/etc/muninn/mcp.env
ExecStart=/usr/bin/mnnn mcp serve --tls-domain mcp.example.com --tls-email you@example.com
KillSignal=SIGINT
Restart=on-failure
AmbientCapabilities=CAP_NET_BIND_SERVICE
CapabilityBoundingSet=CAP_NET_BIND_SERVICE
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/muninn
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

`/etc/muninn/mcp.env` (mode 0600) holds `MUNINN_PASSWORD=…` when the database key is not in
a keyring. Then `sudo systemctl enable --now muninn-mcp` and
`journalctl -u muninn-mcp -f` to watch it get the certificate.

#### Checking it

```
curl -sS https://mcp.example.com/mcp -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
curl -sS -o /dev/null -w '%{http_code}\n' -X POST https://mcp.example.com/mcp   # 401
openssl s_client -connect mcp.example.com:443 -servername mcp.example.com </dev/null 2>/dev/null \
  | openssl x509 -noout -issuer -dates
```

#### When it fails

- **"Let's Encrypt needs a contact address"**: add `--tls-email`.
- **Permission denied on :443 or :80**: see step 3.
- **Address already in use**: another web server holds the port. Stop it, or use
  [nginx in front](mcp-proxy.md) instead.
- **Still self-signed after a minute**: the CA could not reach the challenge. Check DNS,
  that port 80 (or 443 with `tls-alpn-01`) is open and forwarded, and the log
  (failures are logged as warnings; `MUNINN_LOG=info` shows renewals too). Too many failures on production hit Let's Encrypt's rate
  limits; use `--acme-staging` while you fix it.

### HTTPS through nginx

When nginx already serves ports 80 and 443 on the machine, run Muninn on plain HTTP at
127.0.0.1:4743 and let nginx and certbot handle TLS: see [MCP behind nginx](mcp-proxy.md)
and [`packaging/nginx/muninn-mcp.conf`](../packaging/nginx/muninn-mcp.conf).

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
