# Config Reference

Gatebase is configured with one YAML file. See
[`../examples/gatebase.yaml`](../examples/gatebase.yaml) for a complete example.
`gatebase config check --config <path>` loads this file and verifies it can be
parsed. Validation requires at least one target, at least one audit sink, valid
duration fields, and `sessions.default_ttl <= sessions.max_ttl`.

`gatebase config --broker <url>` writes CLI-only settings to
`~/.config/gatebase/config.json`. It does not modify the Gatebase YAML file.

## Top-level sections

| Section | Purpose |
| --- | --- |
| `server` | `public_url` and `broker_listen` address. |
| `admin` | Admin API signing key settings. |
| `metadata` | Metadata/audit/rollback store backend and URL. Defaults to SQLite at `~/.gatebase/gatebase.db`. |
| `sessions` | `default_ttl`, `max_ttl`, and database session `signing_key_file`. |
| `github` | Optional GitHub App credentials. |
| `audit` | `fail_closed` flag and `sinks` (`sqlite`, `jsonl`). |
| `rollback` | Optional before-image rollback artifact capture and sinks. |
| `retention` | Time-based retention windows used by maintenance pruning. |
| `targets` | Database targets: engine, listen address, upstream, credentials. |
| `policies` | Named SQL policies. |

Any YAML string can reference environment variables with `${VAR}`. Gatebase
expands those references before parsing the config and fails startup if a
referenced variable is missing or empty. Use this for secrets such as metadata
Postgres URLs, GitHub webhook secrets, and upstream database credentials.

## `server`

Broker network settings and public URL used in generated connection strings.

| Field | Required | Default | Description |
| --- | --- | --- | --- |
| `public_url` | Yes | None | External broker URL clients and GitHub webhooks use, for example `https://gatebase.example.com`. Its host is also the fallback host in generated database connection strings when a target does not set `public_host`. |
| `broker_listen` | No | `127.0.0.1:8080` | Socket address where `gatebase broker` listens. Use `0.0.0.0:<port>` when binding inside containers or behind a reverse proxy. |

Example:

```yaml
server:
  public_url: "https://gatebase.example.com"
  broker_listen: "127.0.0.1:8080"
```

## `metadata`

Metadata store location. Gatebase stores sessions, access tokens, active
connections, audit events, rollback artifacts, and admin users here. SQLite is
the default single-node backend; Postgres is recommended when broker and proxy
processes run on different hosts or when Kubernetes storage should not depend on
a shared SQLite file.

| Field | Required | Default | Description |
| --- | --- | --- | --- |
| `backend` | No | `sqlite` | Metadata backend. Accepted values are `sqlite` and `postgres`. |
| `url` | No | `sqlite://~/.gatebase/gatebase.db?mode=rwc` | Metadata database URL. Use `sqlite://...` for SQLite or `postgres://...` / `postgresql://...` for Postgres. Supports `${VAR}` expansion. |

Example:

```yaml
metadata:
  backend: "sqlite"
  url: "sqlite:///var/lib/gatebase/gatebase.db?mode=rwc"
```

Postgres metadata example:

```yaml
metadata:
  backend: "postgres"
  url: "${GATEBASE_METADATA_URL}"
```

If `metadata` is omitted, Gatebase uses SQLite at
`~/.gatebase/gatebase.db`. With systemd, `~` belongs to the service user, not
the admin invoking commands.

For broker/proxy split-server deployments, Postgres metadata is preferred. If you
choose SQLite, every process must use the same SQLite file on shared storage;
separate local SQLite files will break session lookup, revocation,
active-connection tracking, and audit/rollback visibility.

## `admin`

Admin API token signing-key settings. This key is separate from session signing
so admin tokens cannot be used as database session tokens.

| Field | Required | Default | Description |
| --- | --- | --- | --- |
| `signing_key_file` | Yes | None | Path to a secret key file used for admin API token signing and verification. |

Example:

```yaml
admin:
  signing_key_file: "/var/lib/gatebase/admin.key"
```

## `sessions`

Session TTL and signing-key settings. The signing key is used to issue database
session tokens that proxies verify when clients connect.

| Field | Required | Default | Description |
| --- | --- | --- | --- |
| `default_ttl` | Yes | None | Default session lifetime, written as a duration string such as `15m`. |
| `max_ttl` | Yes | None | Maximum session lifetime, written as a duration string such as `30m`; `default_ttl` must not exceed it. |
| `signing_key_file` | Yes | None | Path to a secret key file used for session token signing and verification. |

Example:

```yaml
sessions:
  default_ttl: "15m"
  max_ttl: "30m"
  signing_key_file: "/var/lib/gatebase/session.key"
```

Broker and proxies must read the same session signing key content. On split servers,
copy the key securely or mount it from the same secret store. Rotating this key
invalidates existing session tokens unless every process is updated in a
coordinated restart.

## `github`

Optional GitHub App integration. Required when any `github_*` access signal is
configured or when `/webhooks/github` must accept signed GitHub webhooks.

| Field | Required | Default | Description |
| --- | --- | --- | --- |
| `app_id` | Yes, if `github` exists | None | GitHub App ID. |
| `installation_id` | Yes, if `github` exists | None | GitHub App installation ID for the organization or repository. |
| `private_key_file` | Yes, if `github` exists | None | Path to the GitHub App private key PEM file. |
| `webhook_secret` | Yes, if `github` exists | None | Secret used to verify `X-Hub-Signature-256` on GitHub webhooks. |
| `api_base_url` | No | `https://api.github.com` | GitHub API base URL. Override for GitHub Enterprise. |

Example:

```yaml
github:
  app_id: "123456"
  installation_id: 987654
  private_key_file: "/etc/gatebase/github-app.pem"
  webhook_secret: "change-me"
  api_base_url: "https://api.github.com"
```

## `audit`

Audit-write behavior and destinations.

| Field | Required | Default | Description |
| --- | --- | --- | --- |
| `fail_closed` | No | `true` | When `true`, proxy behavior should fail closed if audit writing fails. |
| `sinks` | Yes | None | Audit sink list. At least one sink is required. |

Supported sink types:

| Sink | Fields | Description |
| --- | --- | --- |
| `type: "sqlite"` | None | Writes audit events to the configured metadata store. |
| `type: "jsonl"` | `path` | Appends audit events to a JSONL file at `path`. |

Example:

```yaml
audit:
  fail_closed: true
  sinks:
    - type: "sqlite"
    - type: "jsonl"
      path: "/var/log/gatebase/audit.jsonl"
```

## `rollback`

Optional rollback artifact capture for supported Postgres and MySQL statements.
When enabled, proxies capture before-images for supported `UPDATE` and `DELETE`
statements before forwarding them upstream, then write a rollback artifact to the
configured sinks.

| Field | Required | Default | Description |
| --- | --- | --- | --- |
| `enabled` | No | `false` | Enables rollback artifact capture. When `false`, no rollback sinks are built and no artifacts are written. |
| `max_rows` | No | `100` | Maximum number of before-image rows captured for one statement. Larger matches produce a manual artifact instead of generated inverse SQL. |
| `sinks` | No | `[]` | Rollback artifact sink list. If empty, rollback capture is skipped even when `enabled: true`. |

Supported sink types:

| Sink | Fields | Description |
| --- | --- | --- |
| `type: "sqlite"` | None | Writes rollback artifacts to the configured metadata store. |
| `type: "jsonl"` | `path` | Appends rollback artifacts to a JSONL file at `path`. |

Artifact fields include `session_id`, `actor`, `target`, `engine`, original
`statement`, matched `table`, `primary_key_column`, captured `before_rows`,
optional generated `inverse_sql`, `manual_required`, optional `reason`, and
`created_at`.

Generated inverse SQL is best-effort. Current automatic inverse generation is
limited to `UPDATE` and `DELETE` shapes with `WHERE <primary_key> IN (...)` or
`WHERE <primary_key> = <value>` on a single-column primary key. Schema-qualified
table names are supported. Unsupported shapes, composite/no primary key tables,
or row counts over `max_rows` create artifacts with `manual_required: true`.
Parseable manual artifacts can still include captured `before_rows`; the web UI
can download those rows as CSV.

Rollback sink write failures follow `audit.fail_closed`: when `true`, proxy query
handling fails on sink write errors; when `false`, failures are logged and query
handling continues.

Example:

```yaml
rollback:
  enabled: true
  max_rows: 100
  sinks:
    - type: "sqlite"
    - type: "jsonl"
      path: "/var/log/gatebase/rollback.jsonl"
```

## `retention`

Time-based retention windows used by `gatebase maintenance prune`. Pruning is
explicit: Gatebase does not delete rows automatically during normal broker or
proxy operation.

| Field | Required | Default | Prunes |
| --- | --- | --- | --- |
| `audit_days` | No | `90` | Audit events with `created_at` older than this many days. |
| `rollback_days` | No | `30` | Rollback artifacts with `created_at` older than this many days. |
| `session_days` | No | `30` | Sessions with `expires_at` older than this many days. |
| `approval_days` | No | `30` | Expired one-time access tokens older than this many days. Field name is retained for config compatibility. |
| `active_connection_days` | No | `7` | Closed active-connection rows with `disconnected_at` older than this many days. Active rows are kept. |

Example:

```yaml
retention:
  audit_days: 90
  rollback_days: 30
  session_days: 30
  approval_days: 30
  active_connection_days: 7
```

Run a dry run first:

```bash
gatebase maintenance prune --config examples/gatebase.yaml --dry-run
```

Then prune:

```bash
gatebase maintenance prune --config examples/gatebase.yaml
```

After deleting rows on SQLite, prune runs a WAL checkpoint and `VACUUM` so
SQLite can release disk space. Postgres metadata relies on normal Postgres
maintenance.

## `targets`

Database targets exposed through Gatebase proxies. A session request must name
one of these targets with `--target <name>`.

In split broker/proxy deployments, `listen` is the socket used by the proxy
process on the proxy host. `public_host` and `public_port` are the address and
port clients receive from the broker in generated connection strings.

| Field | Required | Default | Description |
| --- | --- | --- | --- |
| `name` | Yes | None | Stable target name used by sessions, access tokens, audit events, and CLI flags. |
| `engine` | Yes | None | Database engine. Accepted values are `postgres` and `mysql`. |
| `access.github_repo` | Yes | None | GitHub repository tied to this target. Repos must be unique across targets so webhooks infer target from repo. |
| `access.access_token_ttl` | No | `5m` | Lifetime of one-time tokens posted on approved GitHub issues. |
| `access.allow_cli_sessions` | No | `false` | Allow `gatebase session create-local` for this target. |
| `access.required_signals` | No | `[]` | Issue signals required before Gatebase comments a token. Use `github_issue_open` and `github_issue_labels`. |
| `listen` | Yes | None | Socket address where the matching proxy listens for database clients. |
| `public_host` | No | Host from `server.public_url`, then `listen` IP | Host placed in broker-generated connection strings. Useful when proxy listens on `0.0.0.0` or behind DNS. |
| `public_port` | No | `listen` port | Port placed in broker-generated connection strings. Useful behind load balancers or port mappings. |
| `upstream` | Yes | None | Upstream database socket address the proxy forwards allowed queries to. |
| `database` | Yes | None | Database name included in generated connection strings and upstream connection setup. |
| `credentials.username` | Yes | None | Upstream database username. Use `${VAR}` to read from the environment. |
| `credentials.password` | Yes | None | Upstream database password. Use `${VAR}` to read from the environment. |

Example:

```yaml
targets:
  - name: "prod-pg"
    engine: "postgres"
    access:
      github_repo: "org/prod-pg-access"
      access_token_ttl: "5m"
      required_signals:
        - type: "github_issue_open"
        - type: "github_issue_labels"
          labels:
            - "approved"
    listen: "0.0.0.0:15432"
    public_host: "gatebase.example.com"
    public_port: 15432
    upstream: "10.0.0.10:5432"
    database: "app"
    credentials:
      username: "${PG_UPSTREAM_USER}"
      password: "${PG_UPSTREAM_PASSWORD}"
```

## `policies`

Named SQL policy definitions. The MVP proxies read the policy named `default`;
if it is missing, they use an empty config plus built-in dangerous-operation
blocks.

| Field | Required | Default | Description |
| --- | --- | --- | --- |
| `block` | No | `[]` | SQL operation classes to always reject, for example `drop_table` or `truncate`. |
| `require_where` | No | `[]` | SQL operation classes that must include a `WHERE` clause, usually `update` and `delete`. |
| `max_rows_changed` | No | None | Maximum allowed affected rows for one supported mutation. Postgres simple-query and MySQL text-query `INSERT`, `UPDATE`, and `DELETE` statements run in a transaction and roll back if affected rows exceed this value. |

Example:

```yaml
policies:
  default:
    block:
      - "drop_database"
      - "drop_table"
      - "truncate"
      - "alter_system"
    require_where:
      - "update"
      - "delete"
    max_rows_changed: 1000
```
