# Architecture

Gatebase has three runtime parts: broker, proxy, and SQLite metadata store.

```text
configured access signals satisfied
  -> broker validates request
  -> broker creates session
  -> client connects to proxy
  -> proxy validates token
  -> proxy applies policy
  -> proxy forwards to target database
  -> proxy writes audit event
```

## Broker

Broker owns access-signal evaluation, CLI approval creation, GitHub integration, and session issuance. It exposes health endpoints, webhook endpoint, session API, and access approval API.

## Proxy

Proxy owns data-plane enforcement. Postgres simple-query and MySQL text-query paths validate Gatebase session tokens, enforce policy before forwarding statements, and write audit events.

Postgres extended query protocol, TLS, CancelRequest, and native MySQL password-plugin token auth remain future work.

## SQLite

SQLite stores sessions, active connections, CLI approval records, and audit events. Use WAL mode and back up the database like other security records.

## Trust Boundary

Production databases must only accept traffic from Gatebase proxies. If users can bypass Gatebase, audit coverage is incomplete.
