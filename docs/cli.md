# CLI Reference

Gatebase ships one `gatebase` binary for every runtime and operator command. When
running from source, prefix examples with `cargo run -p gatebase-cli --`.

```text
gatebase <command>
```

Most commands take either `--config <path>` or `--broker <url>`:

| Input | Used by | Meaning |
| --- | --- | --- |
| `--config <path>` | Runtime and local metadata commands | Load Gatebase YAML, target settings, session signing key path, and SQLite metadata path. |
| `--broker <url>` | Broker API commands | Send HTTP requests to a running broker. Defaults to `http://127.0.0.1:8080` where supported. |

Set `RUST_LOG` to control CLI and service logging, for example
`RUST_LOG=info gatebase broker --config examples/gatebase.yaml`.

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

## `gatebase session create`

Consumes a one-time access token from a GitHub issue approval. On success it
returns a connection string for a normal database client.

```bash
gatebase session create \
  --broker http://127.0.0.1:8080 \
  --token gb_at_...
```

| Flag | Required | Description |
| --- | --- | --- |
| `--broker <url>` | No | Broker base URL. Defaults to `http://127.0.0.1:8080`. |
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

Lists sessions directly from the SQLite metadata store configured in
`metadata.sqlite_path`. This does not call the broker API.

```bash
gatebase session list --config examples/gatebase.yaml
```

| Flag | Required | Description |
| --- | --- | --- |
| `--config <path>` | Yes | Path to Gatebase YAML config. |

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

Revokes a session directly in the SQLite metadata store. Proxies poll the store
and close matching active connections after revocation is observed.

```bash
gatebase session revoke --config examples/gatebase.yaml <session-id>
```

| Argument or flag | Required | Description |
| --- | --- | --- |
| `--config <path>` | Yes | Path to Gatebase YAML config. |
| `<session-id>` | Yes | Session ID printed by `session create` or `session list`. |

Successful output:

```text
revoked <session-id>
```

The `expires_at` line appears when the broker returns an expiration timestamp.

## `gatebase audit list`

Lists audit events. Use `--broker` from a laptop against a running broker, or
`--config` on the server to read SQLite directly. Exactly one of `--broker` or
`--config` is required.

```bash
gatebase audit list --broker https://gatebase.example.com --target prod-pg --limit 100
gatebase audit list --config /etc/gatebase/gatebase.yaml --decision blocked --json
```

| Flag | Required | Description |
| --- | --- | --- |
| `--broker <url>` | One of broker/config | Broker base URL for remote API mode. |
| `--config <path>` | One of broker/config | Path to Gatebase YAML config for local SQLite mode. |
| `--actor <name>` | No | Filter by actor. |
| `--target <name>` | No | Filter by target. |
| `--decision <allowed|blocked>` | No | Filter by policy decision. |
| `--limit <n>` | No | Maximum events to return. Defaults to `100`. |
| `--json` | No | Print JSON instead of tab-separated output. |

Default output columns are tab-separated:

```text
created_at actor target engine decision rows statement
```

## `gatebase maintenance prune`

Prunes old rows from the SQLite metadata store using the `retention` section in
the config file. This command deletes old audit events, rollback artifacts,
expired sessions, old access tokens, and closed active-connection rows. After a real
prune, it checkpoints WAL and runs `VACUUM` so SQLite can release disk space.

```bash
gatebase maintenance prune --config examples/gatebase.yaml --dry-run
gatebase maintenance prune --config examples/gatebase.yaml
```

| Flag | Required | Description |
| --- | --- | --- |
| `--config <path>` | Yes | Path to Gatebase YAML config. |
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
