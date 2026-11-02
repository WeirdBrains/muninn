# Notifications

Muninn tells you when something happens that you should know about. A notification has a
title, an optional body, a level (`info`, `warn` or `error`) and an optional **link** to
what it is about. Open the link and you land on that review, org, journal, project, rule or
history step.

## Where they come from

- **Built-in events.** Run `mnnn notify kinds` to list them:

  | Kind | When |
  |---|---|
  | `net.exchange.incoming` | another node asks to connect, or invites you |
  | `org.member.apply` | someone applies to join one of your orgs |
  | `review.comment`, `review.suggest`, `review.vote` | a reviewer comments, suggests a change or votes |
  | `sync.pull` | a sync with a linked node kept conflicts |
  | `org.code.sync` | an org's code arrived and needs your approval |
  | `notify` | Muninn warns you (for example, certificates expiring or failing to renew) |
  | `net.broadcast` | another node sent a broadcast |

  Your own actions don't notify you. Network events only notify when they come from
  another node. You can mute any built-in kind with `mnnn notify mute KIND`, or with `m`
  in the window.
- **Rule sets.** Add `notify TITLE [body=…] [link=KIND:ID] [level=info|warn|error]` as a
  then-action. The title, body and link are templates, so `{name}`, `{summary}` and the
  other fields work. Example:

  ```
  mnnn rule create "Big import" --on import --if "inserted > 1000" \
    --then 'notify "imported {inserted} bars" level=warn'
  ```
- **Broadcasts** from other nodes (see below).

## Notifications are triggers too

Every notification is also recorded as a `notification` action, with the fields `id`,
`source`, `level`, `title`, `body` and `link`. That means a rule set can react to any
notification, whatever made it: `--on notification --if "level = error"`. The usual chain
and loop protection applies, so a rule that notifies about a notification won't run forever.

## Reading them

- **TUI.** The header shows a bell (🔔 N unread) when something is unread. Click the
  header, press `!`, or use Projects ▸ Notifications… to open the window:
  - `Enter` opens what the notification links to and marks it read.
  - `r` toggles read or unread on one; `R` marks all read.
  - `u` shows only unread ones.
  - `m` mutes or unmutes that built-in kind.
- **CLI:**
  - `mnnn notify list [--unread] [--json]`
  - `mnnn notify show ID` (also marks it read)
  - `mnnn notify read ID|--all` and `mnnn notify unread ID`
  - `mnnn notify send TITLE [--body] [--link] [--level]`, for scripts
  - `mnnn notify kinds`, `mnnn notify mute KIND`, `mnnn notify unmute KIND`

Notifications are stored additively, in their own table. They sync to your linked nodes,
along with their read state.

## Broadcasts

A broadcast is a short topic (up to 64 characters) plus text (up to 2000) that you send to
other nodes:

- `nodes`: your linked nodes (the default)
- `connections`: nodes you exchanged keys with
- `org:SLUG`: an org's member nodes
- `all`: everyone above

To send one:

```
mnnn net broadcast deploy "v2 is out" --to connections
```

A rule set can broadcast with `broadcast TOPIC [text=…] [to=nodes|connections|all|org:SLUG]`.
On the receiving node a broadcast arrives as a `net.broadcast` action, with the fields
`peer`, `from`, `topic`, `text`, `scope` and `org`. It notifies you unless muted, and rule
sets can trigger on it:

```
mnnn rule create "Deploys" --on net.broadcast --if "topic = deploy" \
  --then 'notify "deploy by {from}" body="{text}"'
```

Broadcasts only travel over active key exchanges, the same consent and trust as all other
network traffic. A node only accepts them over link, connection or org exchanges, and it
checks the size limits.

## Decisions

- **Rule-set broadcasts are queued.** A broadcast from a rule set goes into a node-local
  outbox. The running node (`mnnn net start` or the TUI) sends it on its next settle pass,
  within about 20 seconds. A broadcast that can't be delivered then is dropped and recorded
  as a `net.broadcast.send` action with failures; it is not retried. `mnnn net broadcast`
  sends right away.
- **No "shared with you" notification.** Shares are pulled when you list or fetch them, so
  no event arrives when someone shares with you. Instead, a node can broadcast that it has
  shared something.
- **Your own actions don't notify you.** The exceptions are sync conflicts, org code
  waiting for approval, and Muninn's own warnings.
- **The outbox and mutes stay on this node.** They are settings, so they don't sync.
  Notifications themselves do sync.
