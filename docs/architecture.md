# Architecture

Gatebase has three runtime parts: broker, proxy, and metadata store.

```text
GitHub issue gets required labels
  -> broker webhook validates issue signals
  -> broker comments one-time access token and closes issue
  -> client exchanges token for session
  -> client connects to proxy
  -> proxy validates token
  -> proxy applies policy
  -> proxy forwards to target database
  -> proxy writes audit event
```

## Broker

Broker owns issue access-signal evaluation, GitHub integration, one-time access tokens, session issuance, and admin API authorization. Public endpoints handle health checks, GitHub webhooks, and one-time token exchange. Admin endpoints require a signed bearer token from `/api/admin/login` and enforce a minimum role per endpoint.

| Method & path | Min role | Purpose |
| --- | --- | --- |
| `GET /healthz`, `/readyz` | public | Health checks. |
| `POST /webhooks/github` | public (signature) | GitHub webhook intake. |
| `POST /api/sessions` | public (one-time token) | Exchange an access token for a session. |
| `POST /api/admin/login` | public (password) | Issue an 8-hour bearer token (any role). |
| `GET /api/admin/me` | viewer | Current identity. |
| `GET /api/sessions` | viewer | List sessions. |
| `GET /api/audit/events` | viewer | List audit events. |
| `GET /api/audit/events/:id/rollback` | viewer | Rollback artifact linked to an audit event (404 if none). |
| `GET /api/rollbacks` | viewer | List rollback artifacts. |
| `GET /api/connections` | viewer | List live connections. |
| `GET /api/activity` | viewer | Unified activity feed (audit + rollback + connection events). |
| `POST /api/sessions/:id/revoke` | operator | Revoke a session. |
| `GET /api/admin/users`, `POST /api/admin/users` | admin | List / create users. |
| `POST /api/admin/maintenance/prune` | admin | Prune old metadata. |

The list endpoints (`sessions`, `audit/events`, `rollbacks`, `connections`, `activity`, `admin/users`) accept `limit` and `offset` query parameters for pagination; omitting them returns all rows. Audit events also accept `actor`, `target`, `decision`, and `search` filters; `search` matches actor, target, engine, decision, statement, and error text before pagination. Each audit event carries the id of the rollback artifact captured for that statement, if any.

## Web UI

`gatebase ui` runs a local web server that serves a read-only dashboard and reverse-proxies its API calls to the broker, injecting the operator's saved bearer token. The browser never holds the token; the proxy forwards only `GET` requests on a fixed path allowlist and binds to localhost. Rollback details show generated inverse SQL when available and can download captured `before_rows` as CSV.

## Proxy

Proxy owns data-plane enforcement. Postgres simple-query and MySQL text-query paths validate Gatebase session tokens, enforce policy before forwarding statements, and write audit events.

When rollback capture is enabled, both proxies record best-effort before-images for supported `UPDATE`/`DELETE` statements and link the rollback artifact from the audit event. Automatic inverse SQL is limited to single-column primary-key predicates; parseable manual artifacts can still include captured rows.

Postgres extended query protocol, TLS, CancelRequest, and native MySQL password-plugin token auth remain future work.

## Split Deployment

Broker and proxies can run on different servers. Use this when the broker API is
internet-facing but database proxy ports should live closer to private databases
or approved client networks.

Split deployment requirements:

- Broker and proxies must use the same `sessions.signing_key_file` content so proxies can verify broker-issued session tokens.
- Broker and proxies must use the same metadata store. Postgres metadata is recommended for split hosts. SQLite metadata requires one shared SQLite file path on shared storage, or broker and proxy on the same host. If each server has its own local SQLite file, sessions and revocations do not propagate.
- Proxy servers must reach the configured upstream databases.
- Clients must reach target proxy `public_host` and `public_port` values returned in connection strings.
- Broker `server.public_url` remains the public HTTP URL for webhooks, API, login, and one-time token exchange. It is not necessarily the proxy host.

For split hosts, set each target's `listen` for the proxy server and set
`public_host`/`public_port` to the externally reachable proxy address. Broker uses
those target fields when generating connection strings.

## Metadata Store

The metadata store can be SQLite or Postgres. It stores access tokens, sessions,
active connections, audit events, rollback artifacts, and admin users. Audit
events reference the rollback artifact captured for the statement (when one
exists). User passwords are stored as Argon2 hashes. Back up the metadata store
like other security records.

## Admin RBAC

Admin API roles are ordered `admin > operator > viewer`.

| Role | Permissions |
| --- | --- |
| `viewer` | Read sessions, audit events, rollbacks, connections, activity, and current identity. |
| `operator` | Viewer permissions plus session revoke. |
| `admin` | Operator permissions plus user management and maintenance pruning. |

## Trust Boundary

Production databases must only accept traffic from Gatebase proxies. If users can bypass Gatebase, audit coverage is incomplete.
