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
| Certificate | `mnnn org cert status [--json]`, `mnnn org cert renew` | the Site page |
| ACME | `mnnn org cert acme --domain D --email E [--directory] [--profile] [--http-port]`, `--off` | `e` "domain email" or "off" |
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
