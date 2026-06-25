# CLI Reference

Gatebase ships one `gatebase` binary for every runtime and operator command. When
running from source, prefix examples with `cargo run -p gatebase-cli --`.

```text
gatebase <command>
```

Print version:

```bash
gatebase --version
```

Most commands take either `--config <path>` or `--broker <url>`:

| Input | Used by | Meaning |
| --- | --- | --- |
| `--config <path>` | Runtime and local metadata commands | Load Gatebase YAML, target settings, session signing key path, and metadata store settings. |
| `--broker <url>` | Broker API commands | Send HTTP requests to a running broker. If omitted, commands use the URL saved by `gatebase config --broker <url>` where supported. `session create` falls back to `http://127.0.0.1:8080`. |

Set `RUST_LOG` to control CLI and service logging, for example
`RUST_LOG=info gatebase broker --config examples/gatebase.yaml`.

## `gatebase update`

Downloads a release archive for the current platform from GitHub Releases and
replaces the current `gatebase` executable. If the current executable is not
writable, the command retries with `sudo install -m 0755`.

```bash
gatebase update
gatebase update --version 0.4.5
gatebase update --force
```

| Flag | Required | Description |
| --- | --- | --- |
| `--version <version>` | No | Install a specific release version. Accepts `0.4.5` or `v0.4.5`. Defaults to latest release. |
| `--force` | No | Reinstall even when the requested version matches the current binary. |

Supported release archive names follow
`gatebase-<version>-<target-triple>.tar.gz` for Linux and macOS targets built by
the release workflow.

## `gatebase systemd install`

Writes three systemd service units for the current binary or a supplied binary:

- `gatebase-broker.service`
- `gatebase-proxy-postgres.service`
- `gatebase-proxy-mysql.service`

```bash
gatebase systemd install --config /etc/gatebase/gatebase.yaml
gatebase systemd install --config /etc/gatebase/gatebase.yaml --enable --start
gatebase systemd install --config /etc/gatebase/gatebase.yaml --bin /usr/local/bin/gatebase
```

| Flag | Required | Description |
| --- | --- | --- |
| `--config <path>` | Yes | Gatebase YAML config path used in each `ExecStart`. |
| `--bin <path>` | No | Binary path for `ExecStart`. Defaults to the current executable. |
| `--enable` | No | Run `systemctl enable` for all three units after install. |
| `--start` | No | Run `systemctl start` for all three units after install. With `--enable`, uses `systemctl enable --now`. |

The generated units are minimal `Type=simple` services. For hardened production
units with `User=gatebase`, `EnvironmentFile=...`, and filesystem restrictions,
see [`docs/vps-setup.md`](vps-setup.md).

## `gatebase broker`

Runs the broker HTTP service. The broker validates issue signals, comments
one-time access tokens, creates sessions, receives GitHub webhooks, and exposes
health checks.

```bash
gatebase broker --config examples/gatebase.yaml
```

| Flag | Required | Description |
| --- | --- | --- |
| `--config <path>` | Yes | Path to Gatebase YAML config. |

The broker listens on `server.broker_listen` from the config and uses
`server.public_url` when generating connection strings.

## `gatebase proxy postgres`

Runs the Postgres wire-protocol proxy for targets with `engine: postgres`.
Clients connect to the proxy listen address from the selected target and use the
broker-issued session token as their password.

```bash
gatebase proxy postgres --config examples/gatebase.yaml
```

| Flag | Required | Description |
| --- | --- | --- |
| `--config <path>` | Yes | Path to Gatebase YAML config. |

The proxy validates sessions against SQLite, checks SQL policy before forwarding
queries, writes audit events, and closes connections when sessions expire or are
revoked.

## `gatebase proxy mysql`

Runs the MySQL wire-protocol proxy for targets with `engine: mysql`. Clients must
support clear-password auth toward Gatebase so the session token can be sent to
the proxy.

```bash
gatebase proxy mysql --config examples/gatebase.yaml
```

| Flag | Required | Description |
| --- | --- | --- |
| `--config <path>` | Yes | Path to Gatebase YAML config. |

Like the Postgres proxy, this command validates sessions, applies SQL policy,
writes audit events, and forwards allowed text queries to the upstream database.

## `gatebase config check`

Loads and validates a config file, then exits. Use this in CI or before
restarting services.

```bash
gatebase config check --config examples/gatebase.yaml
```

| Flag | Required | Description |
| --- | --- | --- |
| `--config <path>` | Yes | Path to Gatebase YAML config. |

Successful output:

```text
config ok
```

## `gatebase config --broker`

Stores a default broker URL for remote CLI commands. The setting is written to
`~/.config/gatebase/config.json` and does not modify Gatebase YAML.

```bash
gatebase config --broker https://gatebase.example.com
```

Successful output:

```text
broker https://gatebase.example.com
saved <settings-path>
```

## `gatebase session create`

Consumes a one-time access token from a GitHub issue approval. On success it
returns a connection string for a normal database client.

```bash
gatebase session create \
  --token gb_at_...
```

| Flag | Required | Description |
| --- | --- | --- |
| `--broker <url>` | No | Broker base URL. Defaults to saved broker URL, then `http://127.0.0.1:8080`. |
| `--token <token>` | Yes | One-time token posted by Gatebase on an approved GitHub issue. |

Successful output:

```text
session_id <id>
expires_at <rfc3339 timestamp>
connection_string <database connection string>
```

Use `connection_string` with `psql`, `mysql`, or application drivers that speak
the target protocol.

## `gatebase session list`

Lists sessions. Use `--broker` from a laptop against a running broker, or
`--config` on the server to read SQLite directly. If neither flag is passed, the
saved broker URL is used.

```bash
gatebase session list --config examples/gatebase.yaml
gatebase session list --broker https://gatebase.example.com --admin-token <token>
```

| Flag | Required | Description |
| --- | --- | --- |
| `--config <path>` | No | Path to Gatebase YAML config for local SQLite mode. Cannot be combined with `--broker`. |
| `--broker <url>` | No | Broker base URL for remote API mode. Defaults to saved broker URL. |
| `--admin-token <token>` | Remote mode | Bearer token from `gatebase login`. Optional once you have run `gatebase login`, which saves the token. |

Output columns are tab-separated:

```text
<session_id> <actor> <github_repo_or_-> <issue_or_-> <target> <active|inactive>
```

## `gatebase session create-local`

Creates a session directly from the config/metadata store. This bypasses GitHub
and only works when the target has `allow_cli_sessions: true`.

```bash
gatebase session create-local \
  --config examples/gatebase.yaml \
  --target prod-pg \
  --actor alice
```

| Flag | Required | Description |
| --- | --- | --- |
| `--config <path>` | Yes | Path to Gatebase YAML config. |
| `--target <name>` | Yes | Configured database target. |
| `--actor <name>` | Yes | Actor recorded on session/audit rows. |

## `gatebase session revoke`

Revokes a session. Use `--broker` from a laptop against a running broker, or
`--config` on the server to write SQLite directly. If neither flag is passed, the
saved broker URL is used.

```bash
gatebase session revoke --config examples/gatebase.yaml <session-id>
gatebase session revoke --broker https://gatebase.example.com --admin-token <token> <session-id>
```

| Argument or flag | Required | Description |
| --- | --- | --- |
| `--config <path>` | No | Path to Gatebase YAML config for local SQLite mode. Cannot be combined with `--broker`. |
| `--broker <url>` | No | Broker base URL for remote API mode. Defaults to saved broker URL. |
| `--admin-token <token>` | Remote mode | Bearer token from `gatebase login`. Optional once you have run `gatebase login`, which saves the token. Requires `operator` or higher. |
| `<session-id>` | Yes | Session ID printed by `session create` or `session list`. |

Successful output:

```text
revoked <session-id>
```

The `expires_at` line appears when the broker returns an expiration timestamp.

## `gatebase audit list`

Lists audit events. Use `--broker` from a laptop against a running broker, or
`--config` on the server to read SQLite directly. If neither flag is passed, the
saved broker URL is used.

```bash
gatebase audit list --broker https://gatebase.example.com --admin-token <token> --target prod-pg --limit 100
gatebase audit list --config /etc/gatebase/gatebase.yaml --decision blocked --json
```

| Flag | Required | Description |
| --- | --- | --- |
| `--broker <url>` | No | Broker base URL for remote API mode. Defaults to saved broker URL. |
| `--config <path>` | No | Path to Gatebase YAML config for local SQLite mode. Cannot be combined with `--broker`. |
| `--admin-token <token>` | Remote mode | Bearer token from `gatebase login`. Optional once you have run `gatebase login`, which saves the token. |
| `--actor <name>` | No | Filter by actor. |
| `--target <name>` | No | Filter by target. |
| `--decision <allowed|blocked>` | No | Filter by policy decision. |
| `--limit <n>` | No | Maximum events to return. Defaults to `100`. |
| `--json` | No | Print JSON instead of tab-separated output. |

The broker `GET /api/audit/events` endpoint also accepts `offset` for pagination
and `search` for server-side text search across actor, target, engine, decision,
statement, and error text (used by the web UI); the CLI always requests from
offset 0 and does not expose `search` yet.

Default output columns are tab-separated:

```text
created_at actor target engine decision rows statement
```

## `gatebase maintenance prune`

Prunes old rows from the metadata store using configured retention
windows. Use `--broker` from a laptop against a running broker, or `--config` on
the server to write metadata directly. If neither flag is passed, the saved broker
URL is used. This command deletes old audit events, rollback artifacts, expired
sessions, old access tokens, and closed active-connection rows. After a real
local prune on SQLite, it checkpoints WAL and runs `VACUUM` so SQLite can
release disk space.

```bash
gatebase maintenance prune --config examples/gatebase.yaml --dry-run
gatebase maintenance prune --broker https://gatebase.example.com --admin-token <token> --dry-run
```

| Flag | Required | Description |
| --- | --- | --- |
| `--config <path>` | No | Path to Gatebase YAML config for local metadata mode. Cannot be combined with `--broker`. |
| `--broker <url>` | No | Broker base URL for remote API mode. Defaults to saved broker URL. |
| `--admin-token <token>` | Remote mode | Bearer token from `gatebase login`. Optional once you have run `gatebase login`, which saves the token. Requires `admin`. |
| `--dry-run` | No | Count rows that would be deleted without deleting or vacuuming. |

Output:

```text
would_prune audit_events <count>
would_prune rollback_artifacts <count>
would_prune sessions <count>
would_prune access_tokens <count>
would_prune active_connections <count>
would_prune total <count>
```

Without `--dry-run`, the prefix is `pruned`.

## `gatebase login`

Authenticates against the broker and **saves** the bearer token to
`~/.config/gatebase/config.json`. Subsequent admin commands (`session list`,
`audit list`, `admin user …`, `maintenance prune`) then reuse it automatically,
so you no longer need to pass `--admin-token`.

Login is not admin-only: any role (`viewer`, `operator`, `admin`) can log in.
The role is embedded in the token, and each endpoint enforces its own minimum
role server-side.

```bash
# pipe the password in
printf 'password' | gatebase login --username root --password-stdin

# or omit --password-stdin on a terminal to be prompted (masked input)
gatebase login --username root
```

`--broker` defaults to the URL saved by `gatebase config --broker <url>`.

| Flag | Required | Description |
| --- | --- | --- |
| `--username <name>` | Yes | Broker user to authenticate as. |
| `--broker <url>` | No | Broker base URL. Defaults to the saved broker URL. |
| `--password-stdin` | No | Read the password from stdin instead of prompting. Required when stdin is not a terminal. |

Output:

```text
username <username>
role <role>
saved <settings-path>
```

Tokens expire after 8 hours; run `gatebase login` again to refresh.

## `gatebase ui`

Starts a local web server that serves the read-only dashboard and reverse-proxies
its API calls to the broker, injecting your saved bearer token. The browser never
sees the token. Views: sessions, audit events (with the linked rollback for each
data-changing statement), active connections, users, and a unified activity log.
Rollback details can download captured `before_rows` as CSV when available.

```bash
gatebase login --username root      # saves the token first
gatebase ui                         # serves http://127.0.0.1:7777, opens a browser
```

| Flag | Required | Description |
| --- | --- | --- |
| `--broker <url>` | No | Broker base URL. Defaults to the saved broker URL. |
| `--admin-token <token>` | No | Bearer token. Defaults to the token saved by `gatebase login`. |
| `--port <port>` | No | Local listen port. Defaults to `7777`. |
| `--no-open` | No | Do not open a browser automatically. |

The proxy is read-only: it forwards only `GET` requests on a fixed allowlist of
API paths and binds to localhost.

## `gatebase admin user create`

Creates a broker admin user. Roles are `viewer`, `operator`, and `admin`.

Bootstrap the first admin locally on the broker host:

```bash
printf 'admin-password\n' | gatebase admin user create \
  --config /etc/gatebase/gatebase.yaml \
  --username root \
  --role admin \
  --password-stdin
```

Create later users remotely:

```bash
printf 'new-user-password\n' | gatebase admin user create \
  --broker https://gatebase.example.com \
  --admin-token <token> \
  --username alice \
  --role viewer \
  --password-stdin
```

Create later users locally with admin verification. Stdin contains the existing
admin password, newline, then the new user's password:

```bash
printf 'admin-password\nnew-user-password\n' | gatebase admin user create \
  --config /etc/gatebase/gatebase.yaml \
  --admin-username root \
  --admin-password-stdin \
  --username alice \
  --role viewer \
  --password-stdin
```

## `gatebase admin user list`

Lists admin users. Remote mode requires an admin token. Local mode requires admin
verification after bootstrap.

```bash
gatebase admin user list --broker https://gatebase.example.com --admin-token <token>
printf 'admin-password\n' | gatebase admin user list \
  --config /etc/gatebase/gatebase.yaml \
  --admin-username root \
  --admin-password-stdin
```
