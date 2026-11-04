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
ver=2.18.0
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
