# Deploying a public relay

How the public Muninn relay node is set up on a DigitalOcean droplet. Nodes behind NAT
reserve a slot on it and are reached through it (see [network](network.md#behind-nat)).
The relay's name is `relay.muninn.weirdbrains.com`; its DNS record has to point at the
droplet before anything else works. The same steps set up a relay of your own.

| what | value |
|------|-------|
| node and relay port | 4741, TCP and UDP (QUIC); one port does both |
| ACME HTTP-01 | TCP 80 (`hosting.acme.http_port`) |
| site (optional) | TCP 8443 (`hosting.port`) |
| home | `/var/lib/muninn` (`MUNINN_HOME`) |

## 1. Droplet

- Ubuntu 24.04 LTS, the smallest Basic droplet is enough (a relay forwards, it stores little); add your SSH key
- Reserve a static IP and assign it to the droplet (the DNS record points at it)
- Update it and add a service user with no login:

```sh
apt update && apt upgrade -y
useradd --system --home-dir /var/lib/muninn --create-home --shell /usr/sbin/nologin muninn
```

## 2. DNS

`relay.muninn.weirdbrains.com` must point at the droplet: add `A relay.muninn.weirdbrains.com → <reserved IPv4>` (and `AAAA` if the droplet has IPv6) at the
domain's DNS host. Check with `dig +short relay.muninn.weirdbrains.com` before asking for a
certificate.

## 3. Install Muninn

Use the release `.deb` (prebuilt, no Rust needed; see [install](install.md)), checking it
against `SHA256SUMS`:

```sh
ver=2.22.0
base=https://github.com/WeirdBrains/muninn/releases/download/v$ver
curl -LO $base/muninn_${ver}-1_amd64.deb -LO $base/SHA256SUMS
sha256sum --ignore-missing -c SHA256SUMS
apt install ./muninn_${ver}-1_amd64.deb
mnnn --version
```

## 4. Keys and settings

The node's password goes in a root-only environment file that the unit reads:

```sh
install -d -m 700 /etc/muninn
install -m 600 /dev/null /etc/muninn/relay.env
echo "MUNINN_PASSWORD=$(openssl rand -hex 32)" > /etc/muninn/relay.env
cat /etc/muninn/relay.env                 # copy the password somewhere safe
```

This generates a random 64-character password. Save it in a password manager
or other safe place as well: it unlocks the relay's keys, and without it (or the
24 words from `keys init`) a restored `/var/lib/muninn` can't be opened.

Then, as the service user (`sudo -u muninn env MUNINN_HOME=/var/lib/muninn $(cat /etc/muninn/relay.env) …`
for each command):

```sh
mnnn keys init --non-interactive          # prints the 24 words and shards: keep them offline
mnnn net config --label relay --external relay.muninn.weirdbrains.com:4741 --mdns off --autostart off
mnnn net relay serve on                   # relay traffic for other nodes
mnnn org cert acme --domain relay.muninn.weirdbrains.com --email you@example.com   # Let's Encrypt, HTTP-01 on port 80
mnnn org host autostart on                # the site (and its ACME renewals) runs with the node
```

`--external` puts `/dns4/relay.muninn.weirdbrains.com/tcp/4741` and `…/udp/4741/quic-v1` first in
invites and announces them to peers. In `/var/lib/muninn/preferences.toml`, set
`[hosting] bind = "0.0.0.0"` so Let's Encrypt (port 80) and the site can reach it. The
certificate is renewed by the running site when a third of its life is left
([org hosting](org-hosting.md#certificates)); `mnnn org cert status` shows it.

## 5. systemd unit

The unit ships in the repository as
[`packaging/systemd/muninn-relay.service`](https://github.com/WeirdBrains/muninn/blob/main/packaging/systemd/muninn-relay.service):
it runs `mnnn net start --relay-server --no-mdns` as the `muninn` user with
`MUNINN_HOME=/var/lib/muninn`, reads `MUNINN_PASSWORD` from `/etc/muninn/relay.env`
(root-only, mode 600), may bind port 80 for the Let's Encrypt HTTP-01 challenge, stops
with SIGINT (the same clean shutdown as Ctrl-C) and is sandboxed (`ProtectSystem=strict`,
only `/var/lib/muninn` writable). Install it from a checkout or straight from GitHub:

```sh
install -m 644 packaging/systemd/muninn-relay.service /etc/systemd/system/
# or
curl -fsSL -o /etc/systemd/system/muninn-relay.service \
  https://raw.githubusercontent.com/WeirdBrains/muninn/main/packaging/systemd/muninn-relay.service
systemctl daemon-reload
systemctl enable --now muninn-relay
```

`mnnn net start` runs the node in the foreground until stopped, which is what systemd
wants; there is no separate daemon.

### Set or correct the advertised public address

Before starting the relay, run this as its service account, with the **same
`MUNINN_HOME` and password environment as the service** (see section 4). For the
shipped unit this means user/group `muninn`, `MUNINN_HOME=/var/lib/muninn`, and the
password environment loaded from `/etc/muninn/relay.env`. Do not print the password
or pass it as a command-line argument. Running the command as your login user
without the service's home override changes a different store; restarting the
service does not transfer those settings:

```sh
mnnn net config --clear-external --external relay.muninn.weirdbrains.com:4741
```

`--external` belongs to `net config`, not `net start`. The service's
`mnnn net start --relay-server --no-mdns` uses the saved settings. If it is already
running, restart it after changing the settings: `systemctl restart muninn-relay`.
`--clear-external` removes stale advertised addresses; omit it when deliberately
retaining other valid ones. Repeat `--external` to advertise multiple addresses.
A DNS `host:port` expands to TCP and QUIC multiaddrs; explicit IP or DNS multiaddrs
are also accepted. No peer ID is needed in this setting.

Listening/binding on `0.0.0.0` accepts traffic on local interfaces; it does not name
the public address peers should dial. Advertising a public address does not create
DNS records, forward ports, or open firewalls. Check DNS and allow the relevant
TCP/UDP 4741 traffic separately. Confirm the server's `mnnn --version` and
`mnnn net config --help` before applying instructions from a newer release.

After restarting, run this in the **same service account, home, and password
environment**, not your login user's default home:

```sh
mnnn net status --json
```

A successful restart alone does not confirm that the intended settings are active.
Check that the node is running, then compare `settings.external_addresses` (saved
configuration) with `status.external` (the running node's external multiaddrs).
For the hostname above, the saved value should be
`relay.muninn.weirdbrains.com:4741`; the active addresses should include
`/dns4/relay.muninn.weirdbrains.com/tcp/4741` and
`/dns4/relay.muninn.weirdbrains.com/udp/4741/quic-v1`. Changing saved settings leaves
the already-running node's addresses unchanged until restart; this was tested on
2.22.0. Verify the actual server version rather than assuming an older binary has
the same behavior.

On a fresh client connected to the relay, use `mnnn net peers --json` to inspect
what it actually learns about the relay. Compare those addresses with the intended
public name and current DNS A/AAAA records, and confirm the current relay peer ID.
Saved configuration, active external addresses, and peer-observed advertisements
are separate checks; investigate discrepancies before declaring the setup fixed.

## 6. Firewall

Allow only SSH, the node and relay port, and port 80 for ACME (8443 only if the site
should be public). With `ufw` on the droplet, or the same rules in a DigitalOcean Cloud
Firewall:

```sh
ufw default deny incoming
ufw allow OpenSSH
ufw allow 4741/tcp
ufw allow 4741/udp
ufw allow 80/tcp
ufw allow 4742/tcp   # only if live collaboration (gRPC streams) goes through this node
ufw enable
```

Port 4742/tcp is the live collaboration stream listener (`net.collab_port`); open it only
if peers collaborate with this node directly, otherwise they fall back to polling.

## 7. Using it

Print the relay's address and add it on other nodes:

```sh
sudo -u muninn env MUNINN_HOME=/var/lib/muninn $(cat /etc/muninn/relay.env) mnnn net status
mnnn net relay add /dns4/relay.muninn.weirdbrains.com/tcp/4741/p2p/<PEER ID>   # on each client node
```

The public relay `relay.muninn.weirdbrains.com` has peer ID
`12D3KooWEU3bqaeDhf4X7mf8QsJTsTUxP2vCENaRVMYYfp4mkdxz`, so on any node:

```sh
mnnn net relay add /dns4/relay.muninn.weirdbrains.com/tcp/4741/p2p/12D3KooWEU3bqaeDhf4X7mf8QsJTsTUxP2vCENaRVMYYfp4mkdxz
```

With `--external` set, a relay announces only those addresses. Without it, it announces
only routable listen addresses: private (10/8, 172.16/12, 192.168/16), CGNAT
(100.64/10), link-local, IPv6 ULA (fc00::/7) and loopback ones are never announced,
and circuit addresses through a relay with no routable address are left out of invites.
Set `--external` to exactly the public name or address(es) clients should use
(repeat it for several); a droplet's extra public IPs and an IPv6 address missing from
DNS are then not announced either.

`mnnn org cert acme` uses the HTTP-01 challenge, so port 80/tcp must be open in the
firewall (section 6) while it runs and at every renewal; with port 80 closed it fails.

## 8. Upgrades

```sh
curl -LO $base/muninn_${new}-1_amd64.deb -LO $base/SHA256SUMS && sha256sum --ignore-missing -c SHA256SUMS
apt install ./muninn_${new}-1_amd64.deb
systemctl restart muninn-relay
```

The data in `/var/lib/muninn` stays across upgrades; back it up first with
`mnnn backup create` ([backup and recovery](backup-recovery.md)).

## 9. Logs

```sh
journalctl -u muninn-relay -f            # follow
systemctl status muninn-relay
```

For more detail add `Environment=MUNINN_LOG=debug` (or `libp2p=info,debug`) to the unit
and restart.

## 10. Verify discovery and actual delivery

On each client, using its own intended home, inspect:

```sh
mnnn --version
mnnn net status --json
mnnn net relay list --json
mnnn net peers --json
```

A configured relay alone is not evidence of a live reservation. Check the running
node's active relay connection, `reservations`, and externally usable circuit
addresses ending in `/p2p/<RELAY_ID>/p2p-circuit/p2p/<CLIENT_ID>`. Compare the relay's
current public peer ID from its status with the client configuration. Compare DNS
A/AAAA answers with advertised addresses; stale/private/loopback relay addresses
are not usable by arbitrary internet clients. Do not expose passwords or recovery
words while collecting diagnostics.

Record discovery **before** generating invites: do the two fresh client identities
appear in each other's peer inventory and active connection list? Automatic peer
discovery was observed through the public relay in the November 6, 2026 test; do
not assume that sharing a relay either guarantees discovery or cannot discover
peers. An active transport connection is also distinct from an approved key
exchange, and a saved connected/exchange label does not prove message delivery.

To distinguish discovery from connection approval and delivery, test only between
identities you control:

```sh
# A, with its node running:
mnnn net invite
# B, with its node running:
mnnn net connect '<A invite link>'
# A, if approval is pending:
mnnn net requests
mnnn net accept '<request ID or node name>'
# A:
mnnn msg send '<B peer ID>' 'A-to-B relay test' --json
# B:
mnnn msg send '<A peer ID>' 'B-to-A relay test' --json
# Both:
mnnn msg list --json
```

Invites contain no secret and expire after seven days, but include node identity
and addresses; share them only with the intended tester. Match each sent message
ID to an incoming record on its recipient, not just a successful command or a
persisted peer label. Capture exact errors and transport logs for either direction.

For a cross-network claim, use separate networks and verify the active circuit
transport rather than a direct LAN/VPN/loopback connection. Even an explicit
circuit-target dial can coexist with a direct connection after hole punching;
inspect the actual transport during message delivery. Do not change production
firewalls/routes merely to make a test appear relay-only.

### Dated probe evidence (November 6, 2026, EST)

Two isolated 2.22.0 clients on the same alien-blue computer authenticated the public
relay, obtained reservations, and discovered one another before explicit pairing.
The relay identified itself as 2.20.0 and advertised addresses that differed from
DNS, including private/loopback addresses. Clients logged repeated
`Dropping in-flight connect request because we are at capacity` warnings. These are
investigation leads, not proven causes of cross-network delivery failure; do not
copy the observed transient IP addresses into deployment defaults.

Explicit pairing and messages in both directions succeeded with recipient records,
but direct loopback sockets were present. This test therefore does **not** prove
relay-only or separate-network delivery. Check installed versions, advertised
addresses, circuit selection, and actual bidirectional receipts when reproducing.
