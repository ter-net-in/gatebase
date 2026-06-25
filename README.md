# Gatebase

**Approved, temporary access to production databases.**

[![CI](https://github.com/gatebase/gatebase/actions/workflows/ci.yml/badge.svg)](https://github.com/gatebase/gatebase/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Gatebase is an open-source, platform-agnostic database access gateway. It grants
short-lived, approved access to production databases through a wire-compatible
proxy, records every statement as an audit event, and blocks high-risk SQL by
policy.

Instead of handing out standing credentials, you put Gatebase in front of the
database. A developer opens a GitHub issue in the target's configured access
repo; once the issue is open and has the required labels, Gatebase comments a
short-lived one-time token and closes the issue. The developer exchanges that
token for a normal connection string, and
every query is policy-checked and audited on the way through.

> **Status: early MVP.** The Postgres simple-query proxy, MySQL text-query proxy,
> GitHub issue access tokens, local config-allowed sessions, rollback artifacts,
> web dashboard, self-update, systemd unit generation, and Kubernetes Helm chart
> are implemented. The Postgres extended-query protocol, TLS, and native MySQL
> password-plugin auth are not implemented yet. See [Status & roadmap](#status--roadmap).

## Contents

- [Why Gatebase](#why-gatebase)
- [How it works](#how-it-works)
- [Installation](#installation)
- [Quick start (local demo)](#quick-start-local-demo)
- [CLI reference](#cli-reference)
- [Broker HTTP API](#broker-http-api)
- [Configuration](#configuration)
- [Access signals](#access-signals)
- [SQL policy](#sql-policy)
- [Deployment](#deployment)
- [Development](#development)
- [Status & roadmap](#status--roadmap)
- [Documentation](#documentation)
- [Security](#security)
- [License](#license)

## Why Gatebase

- **Approval-gated access.** The broker issues a session only when configured
  signals pass — for now, GitHub issue open state and required labels.
- **Short-lived by default.** Sessions default to 15 minutes. The proxy closes
  connections when the session expires or is revoked.
- **Audited.** Every allowed and blocked statement is written to SQLite and
  optional JSONL sinks, with actor, target, decision, and reason.
- **Admin RBAC.** Broker admin APIs use SQLite-backed users with `viewer`,
  `operator`, and `admin` roles.
- **Policy enforcement.** High-risk SQL (`DROP`, `TRUNCATE`, `ALTER SYSTEM`,
  unscoped `UPDATE`/`DELETE`, and more) is blocked before it reaches the upstream.
- **Normal client UX.** Clients connect with ordinary connection strings using
  `psql`, `mysql`, and most drivers — no custom client required.
- **Platform-agnostic.** Run it on a VPS with systemd, via Docker Compose, or in
  Kubernetes. A single `gatebase` binary provides every subcommand.

Gatebase only guarantees audit coverage when database network policy forces all
traffic through the proxy. **Users must not retain direct network access to
production databases** — if they can bypass the proxy, audit coverage is
incomplete.

## How it works

```text
1. Developer opens an issue in the target access repo
2. Gatebase webhook validates issue signals and comments a one-time token
3. Developer exchanges the token for a short-lived session
        -> SQLite stores the access token, session, and active connections
        -> developer receives a connection string
4. Developer connects through the Postgres/MySQL proxy
        -> proxy validates the session (exists, not expired, not revoked)
        -> proxy applies SQL policy before forwarding each statement
        -> proxy writes an audit event for every statement
        -> proxy forwards allowed SQL to the target database
```

Gatebase has three runtime parts, all shipped in one binary:

| Part | Responsibility |
| --- | --- |
| **Broker** | Evaluates issue access signals, comments one-time tokens, integrates with GitHub, issues sessions, and enforces admin API RBAC. Exposes the HTTP API. |
| **Proxy** | Data-plane enforcement. Validates session tokens, applies SQL policy, writes audit events, forwards to the upstream database. |
| **SQLite store** | Access tokens, sessions, active connections, audit events, rollback artifacts, and admin users. |

See [`docs/architecture.md`](docs/architecture.md) for more detail.

## Installation

Install system-wide: `curl -fsSL https://raw.githubusercontent.com/ter-net-in/gatebase/main/scripts/install.sh | sh`

Uninstall: `curl -fsSL https://raw.githubusercontent.com/ter-net-in/gatebase/main/scripts/install.sh | sh -s -- --uninstall`

Update an installed binary from the latest GitHub Release:

```bash
gatebase update
```

## Quick start (local demo)

The fastest way to see Gatebase end-to-end is the bundled Docker Compose demo,
which starts the broker, both proxies, and throwaway Postgres and MySQL targets.

```bash
docker compose up --build
```

This uses [`examples/gatebase.compose.yaml`](examples/gatebase.compose.yaml),
generates a session signing key into `./tmp`, and exposes:

- broker on `:8080`
- Postgres proxy on `:15432`
- MySQL proxy on `:13306`

### Running from source

```bash
# Validate a config file
cargo run -p gatebase-cli -- config check --config examples/gatebase.yaml

# Save a default broker URL for remote CLI commands
cargo run -p gatebase-cli -- config --broker http://127.0.0.1:8080

# Start the broker
cargo run -p gatebase-cli -- broker --config examples/gatebase.yaml

# Start the proxies (in separate shells)
cargo run -p gatebase-cli -- proxy postgres --config examples/gatebase.yaml
cargo run -p gatebase-cli -- proxy mysql --config examples/gatebase.yaml
```

The example config requires local files referenced by the YAML:

```bash
mkdir -p tmp
openssl rand -base64 32 > tmp/session.key
touch tmp/github.pem
```

### Requesting a session

For GitHub issue access, Gatebase comments a one-time token on an approved issue
and closes it. A developer consumes that token:

```bash
cargo run -p gatebase-cli -- session create \
  --token gb_at_...
```

For local/admin sessions, enable `allow_cli_sessions: true` on the target and run:

```bash
cargo run -p gatebase-cli -- session create-local \
  --config examples/gatebase.yaml \
  --target prod-pg \
  --actor alice
```

Both commands print `session_id`, `expires_at`, and `connection_string`.

Bootstrap the first admin user locally on the broker host:

```bash
printf 'change-me\n' | cargo run -p gatebase-cli -- admin user create \
  --config examples/gatebase.yaml \
  --username root \
  --role admin \
  --password-stdin
```

### Web dashboard

Log in once (the token is saved to `~/.config/gatebase/config.json`), then open
the read-only dashboard:

```bash
cargo run -p gatebase-cli -- login --username root
cargo run -p gatebase-cli -- ui      # serves http://127.0.0.1:7777
```

`gatebase ui` runs a local server that serves the dashboard and proxies API calls
to the broker with your saved token, so the browser never holds it. After login,
other admin commands (`session list`, `audit list`, …) also reuse the saved token
and no longer need `--admin-token`.

## CLI reference

See [`docs/cli.md`](docs/cli.md) for the full command reference, flags, outputs,
and examples.

## Broker HTTP API

| Method & path | Purpose |
| --- | --- |
| `GET /healthz` | Liveness check. |
| `GET /readyz` | Readiness check. |
| `GET /api/sessions` | List sessions. Requires `viewer` or higher. |
| `POST /api/sessions` | Create a session. Body: `{token}`. Returns `{session_id, expires_at, connection_string}`. |
| `POST /api/sessions/{id}/revoke` | Revoke a session. Requires `operator` or higher. |
| `GET /api/audit/events` | List audit events. Query params: `actor`, `target`, `decision`, `search`, `limit`, `offset`. Requires `viewer` or higher. |
| `GET /api/audit/events/{id}/rollback` | Rollback artifact linked to an audit event (`404` if none). Requires `viewer` or higher. |
| `GET /api/rollbacks` | List rollback artifacts. Requires `viewer` or higher. |
| `GET /api/connections` | List live connections. Requires `viewer` or higher. |
| `GET /api/activity` | Unified activity feed (audit + rollback + connection events). Requires `viewer` or higher. |
| `POST /api/admin/login` | Exchange username/password for an admin bearer token (any role; expires in 8h). |
| `GET /api/admin/me` | Return authenticated admin user. Requires `viewer` or higher. |
| `GET /api/admin/users` | List admin users. Requires `admin`. |
| `POST /api/admin/users` | Create admin user. Requires `admin`. |
| `POST /api/admin/maintenance/prune` | Prune old metadata rows. Requires `admin`. |
| `POST /webhooks/github` | GitHub App webhook intake. Verifies the `X-Hub-Signature-256` HMAC; invalid signatures return `401`. |

List endpoints (`sessions`, `audit/events`, `rollbacks`, `connections`, `activity`, `admin/users`) accept `limit` and `offset` query parameters; omitting them returns all rows.

Example session request:

```bash
curl -sS http://127.0.0.1:8080/api/sessions \
  -H 'content-type: application/json' \
  -d '{"token":"gb_at_..."}'
```

## Configuration

Gatebase is configured with a single YAML file. See
[`examples/gatebase.yaml`](examples/gatebase.yaml) for a complete, commented
example and [`docs/config.md`](docs/config.md) for the full field reference.

Top-level sections:

| Section | Purpose |
| --- | --- |
| `server` | `public_url` and `broker_listen` address; `public_url` host is the fallback target host in generated connection strings. |
| `metadata` | Optional `sqlite_path` for the metadata/audit store. Defaults to `~/.gatebase/gatebase.db`. |
| `sessions` | `default_ttl`, `max_ttl`, and `signing_key_file`. |
| `github` | Optional GitHub App credentials (see below). |
| `audit` | `fail_closed` flag and `sinks` (`sqlite`, `jsonl`). |
| `rollback` | Optional before-image rollback artifact capture and sinks. |
| `retention` | Time-based retention windows used by maintenance pruning. |
| `targets` | Database targets: engine, listen address, upstream, credentials. |
| `policies` | Named SQL policies (see [SQL policy](#sql-policy)). |

Upstream database credentials are read from environment variables named by
`credentials.username_env` and `credentials.password_env`, never stored in the
config file.

## Access signals

Each target has `access.required_signals`. For v1, Gatebase supports GitHub
issue signals only:

| `type` | Behavior |
| --- | --- |
| `github_issue_open` | The GitHub issue must exist and be open. |
| `github_issue_labels` | Every configured label is present on the issue. Takes `labels: [...]`. |

GitHub signals require a configured GitHub App with issue read/write access. See
[`docs/github-app-setup.md`](docs/github-app-setup.md).

## SQL policy

Each entry under `policies` defines what the proxy blocks before forwarding:

```yaml
policies:
  default:
    block:
      - "drop_database"
      - "drop_table"
      - "truncate"
      - "alter_system"
      - "set_global"   # MySQL
      - "load_data"    # MySQL
    require_where:
      - "update"
      - "delete"
    max_rows_changed: 1000
```

- `block` — operation classes that are rejected outright.
- `require_where` — statements that must include a `WHERE` clause.
- `max_rows_changed` — upper bound on affected rows for a single statement.

Multi-statement input is detected and blocked by default. Blocked statements are
never forwarded upstream and are recorded as audit events.

## Deployment

- **VPS / systemd** — [`docs/vps-setup.md`](docs/vps-setup.md) covers the full
  single-node setup: user and directories, secrets, config, systemd units,
  reverse proxy, firewall, and backups.
- **Split broker/proxy hosts** — broker and proxies can run on different servers
  when they share the session signing key and metadata SQLite database. Configure
  target `public_host`/`public_port` to point at the proxy host. See
  [`docs/vps-setup.md`](docs/vps-setup.md#broker-and-proxy-on-different-servers).
- **Generated systemd units** — `gatebase systemd install --config /etc/gatebase/gatebase.yaml --enable --start` writes broker, Postgres proxy, and MySQL proxy units.
- **Docker Compose** — `docker compose up --build` runs the local demo described
  above.
- **Kubernetes / Helm** — [`docs/kubernetes.md`](docs/kubernetes.md) covers the
  Helm chart, Secret setup, proxy Services, Ingress, PVC backups, and
  NetworkPolicy notes.
- **Docker image** — the [`Dockerfile`](Dockerfile) builds a slim image with the
  `gatebase` binary as its entrypoint.

Releases are cut automatically: bump the workspace `version` in `Cargo.toml` and
merge to `main`, and the release workflow tags the version, builds binaries for
Linux and macOS, publishes a GitHub Release, and pushes a multi-arch image to
GHCR. See [`.github/workflows/release.yml`](.github/workflows/release.yml).

## Development

Gatebase is a Rust workspace (minimum Rust 1.88). Before opening a pull request:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
```

Run the opt-in Docker-backed end-to-end tests (Postgres and MySQL happy path,
policy blocking, and audit emission):

```bash
GATEBASE_DOCKER_TESTS=1 cargo test -p gatebase-cli --test docker_e2e -- --nocapture
```

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for contribution guidelines.

## Status & roadmap

**Working today:** broker with GitHub issue access tokens and local config-allowed
sessions, admin RBAC, Postgres simple-query proxy, MySQL text-query proxy (via
`mysql_clear_password`), SQLite sessions/audit/rollback storage, active
connection tracking, rollback artifact capture for supported DML, web dashboard,
SQL policy engine, TTL and revocation enforcement, maintenance pruning, CLI
self-update, systemd unit generation, Docker image/Compose demo, Kubernetes Helm
chart, split broker/proxy deployment docs, and an opt-in Docker E2E test suite.

**Not implemented yet:** Postgres extended-query protocol, TLS, native MySQL
password-plugin auth, GitHub installation-token caching, and richer issue-token
lifecycle controls. Admin user disable/password reset endpoints, admin action
audit events, session disconnect audit reasons, cleaner upstream cancellation for
long-running queries, and broad rollback support for compound predicates,
composite keys, and unsafe/non-unique row identity are also future work.

Gatebase does **not** guarantee universal rollback. Generated rollback artifacts
are best-effort and only safe for constrained DML; WAL/PITR remains the source of
truth for recovery.

The full milestone breakdown lives in
[`docs/implementation-plan.md`](docs/implementation-plan.md).

## Documentation

| Doc | Contents |
| --- | --- |
| [`docs/quickstart.md`](docs/quickstart.md) | Run the broker and proxies locally. |
| [`docs/cli.md`](docs/cli.md) | Full CLI command reference. |
| [`docs/config.md`](docs/config.md) | Full YAML config field reference. |
| [`docs/architecture.md`](docs/architecture.md) | Broker, proxy, and metadata store overview. |
| [`docs/security-model.md`](docs/security-model.md) | Trust assumptions and enforcement model. |
| [`docs/github-app-setup.md`](docs/github-app-setup.md) | Create and configure the GitHub App. |
| [`docs/vps-setup.md`](docs/vps-setup.md) | Single-node VPS deployment with systemd. |
| [`docs/kubernetes.md`](docs/kubernetes.md) | Kubernetes deployment with Helm. |
| [`docs/implementation-plan.md`](docs/implementation-plan.md) | Roadmap and milestones. |

## Security

Gatebase controls database access only when database network policy forces
clients through its proxies. Do not leave direct production database access open
to human users. To report a vulnerability, see [`SECURITY.md`](SECURITY.md).

## License

Apache-2.0. See [`LICENSE`](LICENSE).
