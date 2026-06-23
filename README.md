# Gatebase

**Approved, temporary access to production databases.**

[![CI](https://github.com/gatebase/gatebase/actions/workflows/ci.yml/badge.svg)](https://github.com/gatebase/gatebase/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Gatebase is an open-source, platform-agnostic database access gateway. It grants
short-lived, approved access to production databases through a wire-compatible
proxy, records every statement as an audit event, and blocks high-risk SQL by
policy.

Instead of handing out standing credentials, you put Gatebase in front of the
database. A developer asks the broker for access; the broker only issues a
short-lived session once configured approval signals are satisfied (an approved
GitHub pull request, an operator approval, required CI checks, and so on). The
developer then connects through the proxy with a normal connection string, and
every query is policy-checked and audited on the way through.

> **Status: early MVP.** The Postgres simple-query proxy, MySQL text-query proxy,
> broker-owned approvals, and GitHub App access signals all work and are covered
> by tests. The Postgres extended-query protocol, TLS, and native MySQL
> password-plugin auth are not implemented yet. See [Status & roadmap](#status--roadmap).

## Contents

- [Why Gatebase](#why-gatebase)
- [How it works](#how-it-works)
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
  signals pass — GitHub PR state, approvals, required checks, labels,
  CODEOWNERS-style review, or an operator-created CLI approval.
- **Short-lived by default.** Sessions default to 15 minutes. The proxy closes
  connections when the session expires or is revoked.
- **Audited.** Every allowed and blocked statement is written to SQLite and
  optional JSONL sinks, with actor, target, decision, and reason.
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
1. Developer satisfies configured access signals (e.g. an approved GitHub PR)
2. Broker validates the request and issues a short-lived session
        -> SQLite stores the session, approvals, and active connections
        -> developer receives a connection string + session token
3. Developer connects through the Postgres/MySQL proxy
        -> proxy validates the session (exists, not expired, not revoked)
        -> proxy applies SQL policy before forwarding each statement
        -> proxy writes an audit event for every statement
        -> proxy forwards allowed SQL to the target database
```

Gatebase has three runtime parts, all shipped in one binary:

| Part | Responsibility |
| --- | --- |
| **Broker** | Evaluates access signals, creates CLI approvals, integrates with GitHub, issues sessions. Exposes the HTTP API. |
| **Proxy** | Data-plane enforcement. Validates session tokens, applies SQL policy, writes audit events, forwards to the upstream database. |
| **SQLite store** | Sessions, active connections, CLI approval records, and audit events. |

See [`docs/architecture.md`](docs/architecture.md) for more detail.

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

If the config requires a `cli_approval` signal, an operator first creates an
approval through the broker:

```bash
cargo run -p gatebase-cli -- access approve \
  --broker http://127.0.0.1:8080 \
  --repo gatebase/gatebase \
  --target prod-pg \
  --approver security-oncall \
  --ttl-minutes 30
```

Then a developer requests a session:

```bash
cargo run -p gatebase-cli -- session create \
  --broker http://127.0.0.1:8080 \
  --actor alice \
  --repo gatebase/gatebase \
  --target prod-pg
```

Add `--pull-request 123` when GitHub pull-request signals are required. The
command prints `session_id`, `expires_at`, and a `connection_string` to use with
your database client.

## CLI reference

A single `gatebase` binary provides every runtime and operator command. When
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

### `gatebase broker`

Runs the broker HTTP service. The broker validates access signals, creates
sessions, stores approvals, receives GitHub webhooks, and exposes health checks.

```bash
gatebase broker --config examples/gatebase.yaml
```

| Flag | Required | Description |
| --- | --- | --- |
| `--config <path>` | Yes | Path to Gatebase YAML config. |

The broker listens on `server.broker_listen` from the config and uses
`server.public_url` when generating connection strings.

### `gatebase proxy postgres`

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

### `gatebase proxy mysql`

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

### `gatebase config check`

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

### `gatebase session create`

Requests a short-lived database session from the broker. The broker checks the
configured access signals for the requested repo, pull request, actor, and
target. On success it returns a connection string for a normal database client.

```bash
gatebase session create \
  --broker http://127.0.0.1:8080 \
  --actor alice \
  --repo gatebase/gatebase \
  --pull-request 123 \
  --target prod-pg
```

| Flag | Required | Description |
| --- | --- | --- |
| `--broker <url>` | No | Broker base URL. Defaults to `http://127.0.0.1:8080`. |
| `--actor <name>` | Yes | Person or service requesting access. Must match required approval context. |
| `--repo <owner/name>` | Yes | GitHub repository used for access checks and audit context. |
| `--pull-request <number>` | No | Pull request number. Required when configured GitHub signals need PR context. |
| `--target <name>` | Yes | Configured database target to access, for example `prod-pg`. |

Successful output:

```text
session_id <id>
expires_at <rfc3339 timestamp>
connection_string <database connection string>
```

Use `connection_string` with `psql`, `mysql`, or application drivers that speak
the target protocol.

### `gatebase session list`

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
<session_id> <actor> <repo> <pull_request_or_-> <target> <active|inactive>
```

### `gatebase session revoke`

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

### `gatebase access approve`

Creates a broker-owned CLI approval. Use this when the config contains a
`cli_approval` required signal. The broker stores the approval and later matches
it when `session create` requests the same repo, target, optional actor, and
optional pull request.

```bash
gatebase access approve \
  --broker http://127.0.0.1:8080 \
  --repo gatebase/gatebase \
  --pull-request 123 \
  --target prod-pg \
  --actor alice \
  --approver security-oncall \
  --reason "incident investigation" \
  --ttl-minutes 30
```

| Flag | Required | Description |
| --- | --- | --- |
| `--broker <url>` | No | Broker base URL. Defaults to `http://127.0.0.1:8080`. |
| `--repo <owner/name>` | Yes | GitHub repository covered by the approval. |
| `--pull-request <number>` | No | Pull request covered by the approval. Omit only when `allow_without_pull_request: true` is configured. |
| `--target <name>` | Yes | Configured database target covered by the approval. |
| `--actor <name>` | No | Restrict approval to one actor. Omit to approve any actor that satisfies other rules. |
| `--approver <name>` | Yes | Operator granting approval. Must be allowed by the matching `cli_approval.approvers` config. |
| `--reason <text>` | No | Human-readable reason stored with the approval. |
| `--ttl-minutes <minutes>` | No | Approval lifetime. Broker applies configured default/maximum behavior when omitted or constrained. |

Successful output:

```text
approved <approval-id>
expires_at <rfc3339 timestamp>
```

The `expires_at` line appears when the broker returns an expiration timestamp.

## Broker HTTP API

| Method & path | Purpose |
| --- | --- |
| `GET /healthz` | Liveness check. |
| `GET /readyz` | Readiness check. |
| `GET /api/sessions` | List sessions. |
| `POST /api/sessions` | Create a session. Body: `{actor, repo, pull_request?, target}`. Returns `{session_id, expires_at, connection_string}`. |
| `POST /api/sessions/{id}/revoke` | Revoke a session. |
| `POST /api/access/approvals` | Create a CLI approval. Returns `{approval_id, expires_at?}`. |
| `POST /webhooks/github` | GitHub App webhook intake. Verifies the `X-Hub-Signature-256` HMAC; invalid signatures return `401`. |

Example session request:

```bash
curl -sS http://127.0.0.1:8080/api/sessions \
  -H 'content-type: application/json' \
  -d '{"actor":"alice","repo":"gatebase/gatebase","pull_request":123,"target":"prod-pg"}'
```

## Configuration

Gatebase is configured with a single YAML file. See
[`examples/gatebase.yaml`](examples/gatebase.yaml) for a complete, commented
example. The top-level sections are:

| Section | Purpose |
| --- | --- |
| `server` | `public_url` and `broker_listen` address. |
| `metadata` | `sqlite_path` for the metadata/audit store. |
| `sessions` | `default_ttl`, `max_ttl`, and `signing_key_file`. |
| `github` | Optional GitHub App credentials (see below). |
| `access` | `allowed_repositories` and `required_signals`. |
| `audit` | `fail_closed` flag and `sinks` (`sqlite`, `jsonl`). |
| `targets` | Database targets: engine, listen address, upstream, credentials. |
| `policies` | Named SQL policies (see [SQL policy](#sql-policy)). |

Upstream database credentials are read from environment variables named by
`credentials.username_env` and `credentials.password_env`, never stored in the
config file.

## Access signals

`access.required_signals` is a list of signals that must all pass before the
broker issues a session. Available signal types:

| `type` | Behavior |
| --- | --- |
| `github_pull_request_open` | The referenced PR must exist and be open. |
| `github_pull_request_approved` | At least one reviewer's latest review state is `APPROVED`. |
| `github_checks_passed` | Every configured check run / commit status is successful. Takes `checks: [...]`. |
| `github_labels` | Every configured label is present on the PR. Takes `labels: [...]`. |
| `github_codeowners_reviewed` | Best-effort: no requested reviewers remain and a current approval exists. |
| `manual_approval` | Requires an approval from one of `approvers: [...]`. |
| `cli_approval` | Requires a broker-owned approval from one of `approvers: [...]`. Set `allow_without_pull_request: true` to permit access with no PR. |

GitHub signals require a configured GitHub App and pull-request context. See
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
- **Docker Compose** — `docker compose up --build` runs the local demo described
  above.
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

**Working today:** broker with GitHub App and CLI approval signals, Postgres
simple-query proxy, MySQL text-query proxy (via `mysql_clear_password`), SQLite
sessions and audit, SQL policy engine, TTL and revocation enforcement, and an
opt-in Docker E2E test suite.

**Not implemented yet:** Postgres extended-query protocol, TLS, native MySQL
password-plugin auth, exact CODEOWNERS/team-membership parsing, GitHub
installation-token caching, rollback artifact generation, and richer CLI-approval
lifecycle controls.

Gatebase does **not** guarantee universal rollback. Generated rollback artifacts
are planned as best-effort and only safe for constrained DML; WAL/PITR remains the
source of truth for recovery.

The full milestone breakdown lives in
[`docs/implementation-plan.md`](docs/implementation-plan.md).

## Documentation

| Doc | Contents |
| --- | --- |
| [`docs/quickstart.md`](docs/quickstart.md) | Run the broker and proxies locally. |
| [`docs/architecture.md`](docs/architecture.md) | Broker, proxy, and metadata store overview. |
| [`docs/security-model.md`](docs/security-model.md) | Trust assumptions and enforcement model. |
| [`docs/github-app-setup.md`](docs/github-app-setup.md) | Create and configure the GitHub App. |
| [`docs/vps-setup.md`](docs/vps-setup.md) | Single-node VPS deployment with systemd. |
| [`docs/implementation-plan.md`](docs/implementation-plan.md) | Roadmap and milestones. |

## Security

Gatebase controls database access only when database network policy forces
clients through its proxies. Do not leave direct production database access open
to human users. To report a vulnerability, see [`SECURITY.md`](SECURITY.md).

## License

Apache-2.0. See [`LICENSE`](LICENSE).
