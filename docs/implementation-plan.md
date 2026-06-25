# Gatebase Implementation Plan

This plan tracks work left to turn the current MVP into a usable open-source Gatebase release.

## Current State

Implemented:

- Rust workspace with separated crates.
- Apache-2.0 license.
- SQLite/Postgres metadata-backed session and audit foundations.
- CLI commands for broker, Postgres proxy, MySQL proxy, config check, saved default broker URL, token-backed sessions, local config-allowed sessions, admin user management, self-update, and systemd unit installation.
- Broker HTTP API with public token exchange, GitHub webhook intake, admin login, admin user management, admin maintenance pruning, and RBAC-protected session/audit endpoints.
- Metadata-backed admin users with Argon2 password hashes and `viewer`, `operator`, `admin` roles.
- GitHub provider trait and GitHub App implementation.
- Target-owned access signal policy for GitHub issues and optional `allow_cli_sessions`.
- GitHub App provider creates RS256 App JWTs, fetches installation tokens, verifies webhook HMAC signatures, evaluates issue-open/label signals, comments one-time tokens, and closes approved issues.
- Mocked GitHub API tests cover configured GitHub access signals and denial paths.
- Policy engine with multi-statement detection, high-risk Postgres/MySQL operation blocks, `WHERE` requirements, and decision tests.
- Postgres simple-query proxy with session auth, upstream forwarding, policy, audit, and TTL/revocation checks.
- Proxies store active connections, proactively close active sessions on TTL expiry, and poll for revocation every second during idle and in-flight query handling.
- MySQL text-query proxy MVP using `mysql_clear_password` for Gatebase session token auth, upstream forwarding, policy, audit, and TTL/revocation checks.
- README, architecture, CLI, quickstart, config, VPS, and security docs.
- VPS deployment guide with systemd, reverse proxy, firewall, and operations notes.
- Dockerfile and Docker Compose skeleton.
- Docker Compose local demo config with bundled Postgres/MySQL targets and generated session key.
- Helm chart for Kubernetes deployment with broker/proxy pod, Services, PVC, Secret-backed config, optional Ingress, and optional NetworkPolicy.
- GitHub Actions CI for formatting, Clippy, tests, and `cargo audit`.
- GitHub Actions release workflow that, on a version bump merged to `main`, tags the version, builds Linux/macOS binaries, publishes a GitHub Release, and pushes a multi-arch image to GHCR.
- Opt-in Docker integration test covering Postgres and MySQL proxy happy path, policy blocking, and audit emission.
- Rollback artifact capture for supported Postgres/MySQL `UPDATE` and `DELETE` statements, with generated inverse SQL for single-column primary-key predicates and CSV download of captured before rows in the web UI.
- Web dashboard for sessions, audits, rollback detail, users, connections, and activity.

Not implemented yet:

- GitHub installation-token caching.
- Richer lifecycle controls for issue access tokens, including listing, revocation, and audit events.
- Admin user disable/password reset endpoints and admin action audit events.
- Extended Postgres wire protocol.
- Native MySQL password-plugin token auth; current MVP requires clear-password auth support.
- Session disconnect audit reasons and cleaner upstream cancellation for long-running queries.
- Broad rollback support for compound predicates, composite keys, and unsafe/non-unique row identity.

The phase list below is historical roadmap context; the current-state lists above
are authoritative for what is already implemented.

## Phase 1: Real Postgres Proxy

Goal: allow `psql` and GUI clients to connect through Gatebase to a real Postgres database.

Tasks:

- Implement Postgres `StartupMessage` parsing.
- Handle `SSLRequest` explicitly.
- Extract username/database from startup parameters.
- Authenticate with session token from password field.
- Validate token signature, target, expiry, and revocation status.
- Connect to upstream Postgres with configured service credentials.
- Forward simple query protocol end-to-end.
- Parse client `Query` messages enough to extract SQL text.
- Apply policy before forwarding statements.
- Forward server responses back to the client.
- Capture `CommandComplete` to record affected rows where available.
- Handle `Terminate` cleanly.
- Add connection timeout and upstream timeout.

Acceptance criteria:

- `psql` connects through proxy using a Gatebase connection string.
- `SELECT 1` returns real Postgres result.
- Blocked SQL is not forwarded upstream.
- Allowed SQL is audited.
- Upstream errors are returned to client and audited.

## Phase 2: Session Enforcement

Goal: make short-lived access reliable and revocable.

Tasks:

- Add session lookup by ID in SQLite.
- Reject expired sessions.
- Reject revoked sessions.
- Store active connections in SQLite.
- Disconnect sessions when TTL expires.
- Disconnect sessions when revoked.
- Add `GET /api/sessions`.
- Add `POST /api/sessions/{id}/revoke`.
- Add CLI commands:
  - `gatebase session list`
  - `gatebase session revoke <id>`

Acceptance criteria:

- Expired token cannot connect.
- Revoked token cannot connect.
- Existing connection closes after TTL.
- Existing connection closes after revoke.

## Phase 3: GitHub App Integration

Goal: approved GitHub issues produce short-lived one-time tokens that can create sessions.

Tasks:

- Verify GitHub webhook signatures.
- Implement GitHub App JWT creation.
- Fetch installation token.
- Infer target from webhook repository.
- Validate issue exists and is open.
- Validate required issue labels.
- Comment one-time token on approved issue.
- Close issue after token comment.
- Consume token through `POST /api/sessions`.

Acceptance criteria:

- Unapproved issue does not get a token.
- Approved issue gets a one-time token.
- Consumed token creates a session.
- Invalid webhook signature is rejected.
- Unknown repo is ignored or logged.

## Phase 4: Audit Hardening

Goal: make audit useful for security review and incident response.

Tasks:

- Record real actor from validated session token.
- Record session ID, issue, repo, target, engine, client IP, and timestamp.
- Record statement decision: `allowed` or `blocked`.
- Record policy reason for blocked statements.
- Record rows affected from Postgres `CommandComplete`.
- Record upstream error details without leaking secrets.
- Create parent directory for JSONL audit sink when missing.
- Add configurable fail-open/fail-closed behavior.
- Default production profile to fail closed on audit sink failure.
- Add audit query CLI.

Acceptance criteria:

- Every allowed and blocked SQL statement creates an audit event.
- Audit events contain actor, target, SQL, decision, and timestamp.
- Tokens and upstream passwords never appear in logs or audit events.

## Phase 5: Policy Engine V1

Goal: replace naive string checks with safer Postgres-aware policy.

Tasks:

- Add a Postgres SQL parser or build a constrained classifier around protocol-extracted statements.
- Detect multi-statement input.
- Block multi-statements by default.
- Block high-risk Postgres operations:
  - `DROP DATABASE`
  - `DROP TABLE`
  - `TRUNCATE`
  - `ALTER SYSTEM`
  - `COPY ... PROGRAM`
  - `CREATE EXTENSION`
  - `SECURITY DEFINER`
- Require `WHERE` for `UPDATE` and `DELETE`.
- Add per-target policies.
- Add table-level and operation-level scopes.
- Add policy decision tests.

Acceptance criteria:

- Dangerous SQL is blocked before reaching upstream.
- Policy behavior is covered by tests.
- Policy config is documented with examples.

## Phase 6: SQLite Reliability

Goal: make SQLite safe enough for single-node production deployments.

Tasks:

- Move SQL schema into migration files.
- Run migrations on startup.
- Enable `PRAGMA journal_mode=WAL`.
- Enable `PRAGMA busy_timeout`.
- Add database path parent directory creation.
- Add backup documentation.
- Add integrity check command.

Acceptance criteria:

- Fresh deployment creates metadata DB automatically.
- Concurrent broker/proxy access works under normal load.
- Backup and restore process is documented.

## Phase 7: Deployment

Goal: make Gatebase easy to run on VPS, Docker Compose, Kubernetes, or systemd.

Tasks:

- Fix Docker Compose config paths for container runtime.
- Add local Postgres example target for development.
- Add Helm chart.
- Add Kubernetes `NetworkPolicy` examples.
- Add systemd unit files.
- Add reverse proxy examples for broker API.
- Add health and readiness behavior.
- Add `/metrics` endpoint.

Acceptance criteria:

- Docker Compose quickstart works from clean checkout.
- Helm install starts broker and proxy.
- systemd docs cover binary deployment.

## Phase 8: Postgres Protocol Completeness

Goal: support common clients beyond basic `psql` usage.

Tasks:

- Implement extended query protocol:
  - `Parse`
  - `Bind`
  - `Describe`
  - `Execute`
  - `Sync`
- Track prepared statement SQL.
- Apply policy to prepared statements before execution.
- Handle `CancelRequest`.
- Decide initial `COPY` behavior and block unsupported forms.
- Add TLS support between client and proxy.
- Add TLS support between proxy and upstream.

Acceptance criteria:

- DataGrip/TablePlus/DBeaver basic queries work.
- Prepared statements are audited with original SQL.
- Unsupported protocol features fail closed with clear errors.

## Phase 9: Rollback Artifacts

Goal: generate best-effort rollback artifacts for safe DML only.

Tasks:

- Define rollback artifact schema.
- Inspect primary keys for target tables.
- Support `DELETE FROM table WHERE pk IN (...)`.
- Support `UPDATE table SET ... WHERE pk IN (...)`.
- Capture before-image rows inside transaction.
- Store before rows as JSON.
- Generate inverse SQL.
- Mark unsafe operations as manual rollback required.
- Document rollback limitations clearly.

Acceptance criteria:

- Safe delete creates insert rollback artifact.
- Safe update creates update rollback artifact.
- DDL, cascades, triggers, no-PK tables, and huge changes are marked unsafe.

## Phase 10: MySQL Support

Goal: add MySQL/MariaDB after Postgres is stable.

Tasks:

- Implement MySQL handshake.
- Support common auth plugin path.
- Validate session token from password flow.
- Forward text query protocol.
- Extract SQL and apply policy.
- Audit allowed and blocked statements.
- Add MySQL-specific policy blocks:
  - `SET GLOBAL`
  - `LOAD DATA`
  - unsafe DDL
- Add MySQL docs and support matrix.

Acceptance criteria:

- `mysql` CLI connects through proxy.
- `SELECT 1` works.
- Dangerous SQL is blocked and audited.

## Phase 11: Testing And CI

Goal: prevent regressions in protocol, policy, and security behavior.

Tasks:

- Add `cargo deny`.
- Add integration tests with Docker Postgres.
- Add integration tests with Docker MySQL.
- Test `psql` through proxy.
- Test expired token denial.
- Test revoked session disconnect.
- Test blocked SQL audit.
- Test upstream error audit.
- Fuzz Postgres packet parser.

Acceptance criteria:

- CI blocks formatting, lint, test, dependency, and policy regressions.
- Protocol parser has fuzz target.

## Phase 12: Documentation

Goal: make Gatebase usable and honest about guarantees.

Tasks:

- Full config reference.
- GitHub App setup guide.
- Docker Compose deployment guide.
- Kubernetes deployment guide.
- systemd deployment guide.
- Postgres support matrix.
- MySQL support matrix after implementation.
- Policy guide.
- Audit guide.
- Session lifecycle guide.
- Threat model.
- Operations guide.
- Rollback guide.
- Break-glass access guide.

Acceptance criteria:

- New user can deploy local demo from docs.
- Security team can understand trust boundaries.
- Operators know how to back up SQLite and audit logs.

## Release Milestones

### v0.1

- Real Postgres simple-query proxy.
- SQLite sessions and audit.
- Basic broker API.
- Basic GitHub approval validation.
- Docker Compose quickstart.

### v0.2

- Revocation and TTL disconnect.
- Stronger audit events.
- Policy engine V1.
- Deployment docs.

### v0.3

- Extended Postgres protocol.
- Common GUI client support.
- TLS support.
- Kubernetes Helm chart.

### v0.4

- Rollback artifacts for safe DML.
- Audit query CLI.
- Threat model complete.

### v0.5

- MySQL proxy MVP.
- MySQL docs and tests.

### v1.0

- Stable config schema.
- Hardened Postgres support.
- Security review complete.
- Fuzzing and dependency checks in CI.
- Production deployment docs complete.
