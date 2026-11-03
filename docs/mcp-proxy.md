# MCP behind nginx

Muninn's MCP server can [serve HTTPS itself](mcp.md#https-with-lets-encrypt) with a
Let's Encrypt certificate it gets and renews on its own (`mnnn mcp serve --tls-domain`).
Use that unless nginx already owns ports 80 and 443 on the machine. This guide is the
alternative: Muninn on plain HTTP at 127.0.0.1:4743, nginx in front with a certificate
from certbot. The configuration is [`packaging/nginx/muninn-mcp.conf`](../packaging/nginx/muninn-mcp.conf).

Muninn needs no proxy settings. It checks every request's client token
(`Authorization: Bearer mnnn_mcp_…`, passed through unchanged) and builds no URLs from
the `Host` header. It answers only `POST /mcp`, with JSON: no server-sent event stream and
no session, so no WebSocket or SSE settings are needed. Requests with an `Origin` other
than localhost get `403`, so browser-based clients can't call it through the proxy;
desktop and server-side MCP clients send no `Origin`.

Replace `mcp.example.com` and `you@example.com` throughout.

## 1. DNS

Add an `A` record (and `AAAA` if the machine has IPv6) for `mcp.example.com` pointing at
the machine. Check it before going on, since Let's Encrypt validates by name:

```
dig +short mcp.example.com A
dig +short mcp.example.com AAAA
```

## 2. Install nginx and certbot

```
# Debian, Ubuntu
sudo apt install nginx certbot python3-certbot-nginx
# Fedora
sudo dnf install nginx certbot python3-certbot-nginx
sudo systemctl enable --now nginx
# Arch
sudo pacman -S nginx certbot certbot-nginx
sudo systemctl enable --now nginx
```

## 3. Firewall

Open 80 (challenges and the redirect) and 443. Keep 4743 closed: Muninn listens on
127.0.0.1 only, and nothing outside should reach it.

```
# ufw (Debian, Ubuntu)
sudo ufw allow 80/tcp && sudo ufw allow 443/tcp
# firewalld (Fedora)
sudo firewall-cmd --permanent --add-service=http --add-service=https && sudo firewall-cmd --reload
```

## 4. Muninn as a service

Create a user, a home and a client token as that user:

```
sudo useradd --system --home /var/lib/muninn --create-home muninn
sudo -u muninn MUNINN_HOME=/var/lib/muninn mnnn mcp client add assistant --role read
```

The token is printed once; keep it for the client. Then the unit:

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
EnvironmentFile=-/etc/muninn/mcp.env
ExecStart=/usr/bin/mnnn mcp serve --http 127.0.0.1:4743
KillSignal=SIGINT
Restart=on-failure
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/muninn
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

`/etc/muninn/mcp.env` (mode 0600, owned by root) holds `MUNINN_PASSWORD=…` when the
database key is not in a keyring. Start it and check it answers locally:

```
sudo systemctl daemon-reload && sudo systemctl enable --now muninn-mcp
curl -s -o /dev/null -w '%{http_code}\n' -X POST http://127.0.0.1:4743/mcp   # 401: no token
```

## 5. nginx and the first certificate

nginx refuses to start with a missing certificate, so the first one comes before the
HTTPS server block is turned on.

```
sudo mkdir -p /var/www/letsencrypt
# Debian, Ubuntu
sudo cp packaging/nginx/muninn-mcp.conf /etc/nginx/sites-available/muninn-mcp.conf
sudo ln -s ../sites-available/muninn-mcp.conf /etc/nginx/sites-enabled/
# Fedora, Arch (Arch: make sure nginx.conf has `include /etc/nginx/conf.d/*.conf;` in http {})
sudo cp packaging/nginx/muninn-mcp.conf /etc/nginx/conf.d/muninn-mcp.conf
```

Edit the copy: replace `mcp.example.com` (5 places) and comment out the whole
`listen 443` server block. Then:

```
sudo nginx -t && sudo systemctl reload nginx
sudo certbot certonly --webroot -w /var/www/letsencrypt -d mcp.example.com \
  --email you@example.com --agree-tos --no-eff-email
```

Uncomment the 443 block, then `sudo nginx -t && sudo systemctl reload nginx`.

`certbot --nginx -d mcp.example.com` works too, but it edits the file itself. The webroot
way keeps the shipped file as it is.

Add `--staging` to the certbot command to try it out against Let's Encrypt's staging CA
first (untrusted certificate, generous rate limits), then run it again without
`--staging` and with `--force-renewal`.

## 6. Renewal

The certbot package installs a timer (`systemctl list-timers | grep certbot`; on Arch,
enable it: `sudo systemctl enable --now certbot-renew.timer`). nginx must reload to serve
a renewed certificate:

```
sudo tee /etc/letsencrypt/renewal-hooks/deploy/reload-nginx.sh >/dev/null <<'SH'
#!/bin/sh
systemctl reload nginx
SH
sudo chmod +x /etc/letsencrypt/renewal-hooks/deploy/reload-nginx.sh
sudo certbot renew --dry-run
```

## 7. Check it

```
TOKEN=mnnn_mcp_…
curl -sS https://mcp.example.com/mcp -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
curl -s -o /dev/null -w '%{http_code}\n' -X POST https://mcp.example.com/mcp      # 401
curl -s -o /dev/null -w '%{http_code}\n' http://mcp.example.com/mcp               # 301
curl -s -o /dev/null -w '%{http_code}\n' https://mcp.example.com/                 # 404
```

Clients use `https://mcp.example.com/mcp` with the token as a bearer token, for example
`mnnn mcp add home --url https://mcp.example.com/mcp --token -` on another Muninn.

## Troubleshooting

- **`502 Bad Gateway`**: Muninn isn't running or isn't on 127.0.0.1:4743. Check
  `systemctl status muninn-mcp` and `journalctl -u muninn-mcp`.
- **`401`**: the token is missing, wrong or revoked (`mnnn mcp client list`). nginx passes
  `Authorization` through; check no other config strips it.
- **`403`**: the request carried a browser `Origin`. Use a non-browser client.
- **`405`**: a `GET` (or `DELETE`). Muninn keeps no stream or session; clients fall back to
  `POST`.
- **`413`**: the request is over 4 MiB, Muninn's limit (nginx's `client_max_body_size 4m`
  matches it).
- **certbot "Timeout during connect" or "unauthorized"**: DNS doesn't point here yet, port
  80 is closed, or the challenge `location` isn't served (`curl
  http://mcp.example.com/.well-known/acme-challenge/x` should give nginx's 404, not a
  redirect).
- **nginx won't start, certificate file missing**: the 443 block was on before the first
  certificate; see step 5.
- **Muninn fails at start without a prompt**: it runs non-interactively; put the database
  password in `/etc/muninn/mcp.env` or the keyring.
