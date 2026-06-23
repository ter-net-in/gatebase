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

Create a local/admin session if the target has `allow_cli_sessions: true`:

```bash
cargo run -p gatebase-cli -- session create-local \
  --config examples/gatebase.yaml \
  --target prod-pg \
  --actor alice
```

Create a session through the broker from a GitHub issue token:

```bash
cargo run -p gatebase-cli -- session create \
  --broker http://127.0.0.1:8080 \
  --token gb_at_...
```

Run Docker-backed E2E tests explicitly:

```bash
GATEBASE_DOCKER_TESTS=1 cargo test -p gatebase-cli --test docker_e2e -- --nocapture
```
