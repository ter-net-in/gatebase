# Quickstart

Create local files:

```bash
mkdir -p tmp
openssl rand -base64 32 > tmp/session.key
touch tmp/github.pem
```

Check config:

```bash
cargo run -p gatebase-cli -- config check --config examples/gatebase.yaml
```

Run broker:

```bash
cargo run -p gatebase-cli -- broker --config examples/gatebase.yaml
```

Run Postgres proxy:

```bash
cargo run -p gatebase-cli -- proxy postgres --config examples/gatebase.yaml
```

Run MySQL proxy:

```bash
cargo run -p gatebase-cli -- proxy mysql --config examples/gatebase.yaml
```

Create a CLI approval through the broker when `cli_approval` is required:

```bash
cargo run -p gatebase-cli -- access approve \
  --broker http://127.0.0.1:8080 \
  --repo gatebase/gatebase \
  --target prod-pg \
  --approver security-oncall \
  --ttl-minutes 30
```

If approval must not be tied to a pull request, the broker config must explicitly set:

```yaml
- type: "cli_approval"
  approvers:
    - "security-oncall"
  allow_without_pull_request: true
```

Create a session through the broker:

```bash
cargo run -p gatebase-cli -- session create \
  --broker http://127.0.0.1:8080 \
  --actor alice \
  --repo gatebase/gatebase \
  --target prod-pg
```

Add `--pull-request 123` when GitHub pull-request signals are required.

Run Docker-backed E2E tests explicitly:

```bash
GATEBASE_DOCKER_TESTS=1 cargo test -p gatebase-cli --test docker_e2e -- --nocapture
```
