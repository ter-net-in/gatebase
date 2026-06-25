# Kubernetes Deployment

Gatebase ships a Helm chart in [`../charts/gatebase`](../charts/gatebase). The
chart runs the broker and enabled database proxies in one pod by default. SQLite
metadata uses a `ReadWriteOnce` volume; Postgres metadata can use an external
Postgres database shared by all Gatebase containers.

## Requirements

- Kubernetes cluster with a default `StorageClass`, or set
  `persistence.storageClass`.
- Helm 3.
- A Kubernetes Secret containing the session signing key and upstream database
  credentials for production deployments.

Create the production Secret:

```bash
kubectl create secret generic gatebase-secrets \
  --from-literal=session.key="$(openssl rand -base64 32)" \
  --from-literal=PG_UPSTREAM_USER="gatebase" \
  --from-literal=PG_UPSTREAM_PASSWORD="change-me"
```

If GitHub access signals are enabled, add the GitHub App private key to the same
Secret:

```bash
kubectl create secret generic gatebase-secrets \
  --from-file=github-app.pem=./github-app.pem \
  --from-literal=session.key="$(openssl rand -base64 32)" \
  --from-literal=PG_UPSTREAM_USER="gatebase" \
  --from-literal=PG_UPSTREAM_PASSWORD="change-me"
```

The session key must be stable. Rotating it invalidates existing session tokens.

## Minimal Values

Create `values.gatebase.yaml`:

```yaml
image:
  repository: ghcr.io/ter-net-in/gatebase
  tag: "0.4.5"

secrets:
  existingSecret: gatebase-secrets

broker:
  ingress:
    enabled: true
    className: nginx
    hosts:
      - host: gatebase.example.com
        paths:
          - path: /
            pathType: Prefix
    tls:
      - secretName: gatebase-tls
        hosts:
          - gatebase.example.com

postgresProxy:
  enabled: true
  service:
    type: LoadBalancer
    port: 15432

mysqlProxy:
  enabled: false

config:
  server:
    public_url: "https://gatebase.example.com"
    broker_listen: "0.0.0.0:8080"
  metadata:
    backend: "sqlite"
    url: "sqlite:///var/lib/gatebase/gatebase.db?mode=rwc"
  sessions:
    default_ttl: "15m"
    max_ttl: "30m"
    signing_key_file: "/etc/gatebase/secrets/session.key"
  audit:
    fail_closed: true
    sinks:
      - type: "sqlite"
      - type: "jsonl"
        path: "/var/lib/gatebase/audit.jsonl"
  targets:
    - name: "prod-pg"
      engine: "postgres"
      access:
        github_repo: "org/prod-pg-access"
        required_signals:
          - type: "github_issue_open"
          - type: "github_issue_labels"
            labels:
              - "approved"
      listen: "0.0.0.0:15432"
      public_host: "gatebase-postgres.example.com"
      public_port: 15432
      upstream: "postgres.default.svc.cluster.local:5432"
      database: "app"
      credentials:
        username: "${PG_UPSTREAM_USER}"
        password: "${PG_UPSTREAM_PASSWORD}"
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

Install:

```bash
helm upgrade --install gatebase ./charts/gatebase -f values.gatebase.yaml
```

Check rollout:

```bash
kubectl rollout status deployment/gatebase
kubectl get svc gatebase-broker gatebase-postgres
```

## Broker Access

Expose the broker with Ingress. GitHub webhooks and the CLI use
`config.server.public_url`.

Health endpoints:

```bash
curl https://gatebase.example.com/healthz
curl https://gatebase.example.com/readyz
```

Current readiness checks broker HTTP availability. Metadata dependency checks are
future hardening work.

## Proxy Access

For production, expose database proxies with a private `LoadBalancer`, internal
DNS, VPN, or port-forward. Production databases must accept traffic only from
Gatebase proxy pods.

Temporary local port-forward:

```bash
kubectl port-forward svc/gatebase-postgres 15432:15432
```

## Persistence

The chart stores Gatebase metadata and JSONL audit/rollback files on one PVC at
`/var/lib/gatebase`.

Back up the PVC like other security records when using SQLite metadata. SQLite
contains sessions, active connections, audit events, rollback artifacts, access
tokens, and admin users.

The chart defaults to one pod replica and `Recreate` rollout strategy. Do not
scale Gatebase horizontally while SQLite is the metadata store.

To use external Postgres metadata instead, keep
`GATEBASE_METADATA_URL` in the Secret and override only the metadata block:

```bash
kubectl create secret generic gatebase-secrets \
  --from-literal=session.key="$(openssl rand -base64 32)" \
  --from-literal=GATEBASE_METADATA_URL="postgres://gatebase:change-me@postgres.default.svc.cluster.local:5432/gatebase" \
  --from-literal=PG_UPSTREAM_USER="gatebase" \
  --from-literal=PG_UPSTREAM_PASSWORD="change-me"
```

```yaml
config:
  metadata:
    backend: "postgres"
    url: "${GATEBASE_METADATA_URL}"
```

With Postgres metadata, the PVC is still needed if JSONL audit or rollback sinks
write to `/var/lib/gatebase`; otherwise it can be disabled with
`persistence.enabled=false`.

## NetworkPolicy

Set `networkPolicy.enabled=true` to create a baseline policy. Use
`brokerIngressFrom`, `proxyIngressFrom`, and `egress` to restrict traffic for
your cluster CNI.

Example allowing only same-namespace ingress:

```yaml
networkPolicy:
  enabled: true
  brokerIngressFrom:
    - podSelector: {}
  proxyIngressFrom:
    - podSelector: {}
```

Most production clusters should also restrict egress to DNS, GitHub API, and the
upstream databases.

## Secrets And Rotation

The chart can generate a demo `session.key` when `secrets.existingSecret` and
`secrets.sessionKey` are empty. Production should always provide an existing
Secret so upgrades do not change the key unexpectedly.

Session key rotation invalidates existing session tokens. Plan a maintenance
window until multi-key verification exists.
