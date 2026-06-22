# Contributing

Run before opening a pull request:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
```

Avoid logging secrets, session tokens, or upstream database credentials.
