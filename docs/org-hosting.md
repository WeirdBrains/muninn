# Org hosting

Any node can serve the sites of its organizations over HTTPS: HTML pages, a JSON API,
template assets and gRPC, all on one port. **Nothing listens until you start it** (or
turn on `hosting.autostart`: `mnnn org host autostart on`, TUI Net ▸ Node ▸ Settings ▸ *web
server with the node*, so it comes up whenever the node does), and by default only on
`127.0.0.1`. Which orgs are served is each org's *hosted* flag
(`mnnn org edit ORG --hosted on`), plus mirrors of joined orgs.

| | CLI | TUI (Orgs ▸ Site & certificate…) |
|---|---|---|
| Start | `mnnn org host start [--bind ADDR] [--port N] [--for SECS]` (foreground, Ctrl-C stops) | `S` / Enter, or Orgs ▸ Start site |
| Stop | `mnnn org host stop` | `S` again or `X`, or Orgs ▸ Stop site |
| Status | `mnnn org host status [--json]` | the Site page |
| Certificate | `mnnn org cert status [ORG] [--json]`, `mnnn org cert renew [--org ORG]` | the Site page |
| ACME | `mnnn org cert acme --domain D --email E [--directory] [--profile] [--http-port] [--challenge http-01\|tls-alpn-01]`, `--off` | `e` "domain email" or "off" |
| Org domains | `mnnn org domain add\|list\|remove ORG DOMAIN` | Organizations page `w` (add D / remove D) |
| Org certificate | `mnnn org cert set ORG --cert FILE --key FILE`, `mnnn org cert clear ORG`, `mnnn org cert acme --org ORG [--email E] [--off]` | Organizations page `c` ("CERT.pem KEY.pem" sets or replaces, "clear"), `L` ("on [email]" / "off"); the Site page lists each org's |
| Mirror | `mnnn org mirror ORG` (a joined org's public pages; needs `host` there) | Joined page `m` |

Settings live under `[hosting]` in the [preferences](preferences.md): `bind`
(`127.0.0.1`; `0.0.0.0` or `::` serves other machines), `port` (8443), `names` (extra
names for the self-signed certificate) and `cert_days` (7).

## Certificates

- **Self-signed** (default): a new short-lived certificate is made at each start, for
  `localhost`, the bind address and `names`.
- **ACME / Let's Encrypt**: opt-in with a domain and an email. Muninn's pure-Rust ACME
  client asks for a `shortlived` certificate (about six days; falls back to the CA's
  default profile) through the HTTP-01 challenge, answered on `http_port` (80; forward
  port 80 there if needed). It is renewed when a third of its life is left (checked
  every ten minutes while the site runs). The ACME account key and certificates are kept
  in the encrypted database, never in files. `directory` is `letsencrypt`,
  `letsencrypt-staging` or a directory URL. `mnnn org cert renew` renews now (not while
  a site runs: a running site renews itself).

## Org domains and certificates (2.16)

An org may be served at its own domains (`mnnn org domain add lab lab.example.org`; point
the name's DNS at this machine). The one site port serves every org: the certificate is
picked by the name the browser asks for (SNI), and a request for the bare domain root
goes to the org's site (`/o/SLUG/`). Each org's certificate comes from, in order:

1. **Yours**: `mnnn org cert set ORG --cert chain.pem --key key.pem`. The chain and key
   are checked (PEM, the key matches the leaf, not expired) and kept in the encrypted
   database; the key is never logged or written out. `mnnn org cert clear ORG` stops
   using it.
2. **Let's Encrypt**: `mnnn org cert acme --org ORG --email E` (the email may be the
   node's, `mnnn org cert acme --email E`). One certificate covers all the org's domains.
   The node's ACME settings apply: directory, profile and challenge, `http-01` (port 80
   must reach `http_port`) or `tls-alpn-01` (port 443 must reach the site port; set with
   `--challenge`). Behind NAT, where neither port is reachable, DNS-01 is the usual way;
   it is not built in: get a certificate elsewhere and set it as yours.
3. **Self-signed** for the org's domains, made when the site starts.

`mnnn org cert status` lists each org with where its certificate came from, its names and
when it expires (days left), plus any ACME failures. A running site renews ACME
certificates by itself (checked every minute; due when a third of the life is left,
which is about 30 days for a 90-day certificate) and picks up domains and certificates
set from another process within a minute. Failures notify (`notify`, `about =
cert.renew`) and retry with backoff: an hour, doubling, at most a day. Your own
certificates are not renewed by Muninn; the [expiry watcher](rules.md#scheduled-rule-sets)
reminds you.

## Paths

| path | what |
|------|------|
| `/` | the public orgs served here |
| `/o/SLUG/` | an org's page: its published artifacts and endpoints |
| `/o/SLUG/a/ID[.FORMAT]` | an artifact: `html`, `json`, `pdf`, `docx`, `tex`, `txt` or `mnnp` as its kind allows |
| `/o/SLUG/assets/NAME` | template assets |
| `GET /o/SLUG/api/org`, `/api/artifacts`, `/api/endpoints` | JSON |
| `POST /o/SLUG/api/call/ENDPOINT` | call an endpoint with a JSON body |

Guests see what is published `public`; signed members also see what is published to
`members`, and call endpoints their role allows. Pages carry `nosniff`, `no-referrer`,
a same-origin Content-Security-Policy and, with ACME, HSTS. Bodies are capped at 8 MiB.

## Signed requests

Member sign-in is on by default (`mnnn org edit ORG --member-auth off` turns it off):
members authenticate with the key of their org key exchange:

```
Authorization: Muninn exchange=ID, ts=MS, sig=HEX
sig = HMAC-SHA256(HKDF-SHA256(exchange key, info "muninn/org/web/v1"),
                  "METHOD\nPATH?QUERY\nTS\nhex(SHA256(BODY))")
```

Requests more than five minutes off, or replayed, are refused. For gRPC the body is the
length-prefixed frame. `mnnn org sign ORG METHOD PATH [--body FILE]` prints the header
for a joined org (the key never leaves the encrypted database).

## gRPC

Unary calls on the same port (HTTP/2, `application/grpc`, uncompressed):

```proto
syntax = "proto3";
package muninn.org.v1;

service Org {
  rpc GetOrg(OrgRequest) returns (JsonReply);
  rpc ListArtifacts(OrgRequest) returns (JsonReply);
  rpc ListEndpoints(OrgRequest) returns (JsonReply);
  rpc GetArtifact(ArtifactRequest) returns (ArtifactReply);
  rpc Call(CallRequest) returns (JsonReply);
}

message OrgRequest { string org = 1; }                      // slug or id
message ArtifactRequest { string org = 1; string id = 2; string format = 3; } // json by default
message CallRequest { string org = 1; string endpoint = 2; string input_json = 3; }
message JsonReply { string json = 1; }
message ArtifactReply { string mime = 1; bytes data = 2; }
```

Errors use the usual gRPC status codes (`UNAUTHENTICATED`, `PERMISSION_DENIED`,
`NOT_FOUND`, `INVALID_ARGUMENT`).

## Executables (2.13)

Every code space a node serves (`mnnn org code serve ORG SPACE`) is an *executable*,
described in protobuf and callable by anyone holding `code.run` in the org:

| What | Web | gRPC |
|------|-----|------|
| list | `GET /o/ORG/api/executables` | `muninn.code.v1.Executables/List` (`ListRequest{org}`) |
| run  | `POST /o/ORG/api/run/NAME` (JSON) | `muninn.code.v1.o.ORG.Name/Run` |
| protos | `GET /o/ORG/proto/muninn/code/v1/executable.proto`, `GET /o/ORG/proto/Name.proto` | |

The input is the space's own record query, narrowed: `where` adds conditions to the
space's, `text`, `sort` and `group` replace them, and `limit` can never raise the space's.
A run returns a `RunResult` (status, error, the `@view` the source declares, logs, writes,
ms, and the full JSON). The node runs only the source it has approved; an edited space must
be approved again before it runs. Every run is recorded as an `org.run` action.

## Gated private sites (2.13)

A private org's site shows guests nothing. Turn on its door with
`mnnn org edit ORG --gated on`: guests then see a landing page (name and description),
`/o/ORG/sign-in` and `/o/ORG/join`; everything else stays for members.

**Browser sign-in.** A member runs `mnnn org sign-in ORG --site https://HOST:PORT` on
their own node. It prints a link signed with their exchange key, good once and for five
minutes. Opening it sets an `HttpOnly; Secure; SameSite=Strict` cookie for that org's
pages for 12 hours. The node keeps only a hash of the session, in memory, so restarting the
host signs everyone out. `/o/ORG/sign-out` ends a session early. Sessions cover GET pages;
API calls stay signed requests.

**Invite links.** When the site is being served, `mnnn org member invite` also prints a web
link, `https://HOST/o/ORG/join#muninn://join/…`. The invite stays in the `#fragment`,
which browsers never send, so it never reaches the host or its logs. The join page reads
it in the browser and offers to open it in Muninn, or shows the `mnnn org join` command.
Accepting the invite starts the key exchange from the invitee's node.

## Built-in executables (2.13)

Muninn ships global executables, versioned with it, that every hosted org serves to
anyone, including guests at a gated door:

| name | what |
|------|------|
| `invite-redeem` | `{ "link": … }` → is this invite for this org, still open, and which role |
| `membership-apply` | how to apply: the org node's peer id, addresses, and the `mnnn org apply` command |

Use `mnnn code builtin list` to see them and `mnnn code builtin show NAME` for a contract.
`mnnn code builtin override NAME ORG` asks first, then creates an editable code space of
the same name and serves it in place of the built-in. An override runs as ordinary code,
so callers need `code.run` and the source must be approved.
