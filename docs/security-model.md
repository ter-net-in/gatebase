# Security Model

Gatebase assumes:

- users satisfy configured broker access signals before receiving session tokens
- GitHub-specific access signals use GitHub issues and short-lived one-time tokens
- local CLI sessions are allowed only when target config sets `allow_cli_sessions: true`
- production database network access is restricted to Gatebase proxies
- audit sinks are protected from modification by normal users
- backups and PITR/WAL remain enabled for real recovery

Session enforcement happens at connection auth and during proxy activity. Proxies record active connections, close them when the session TTL expires, and poll SQLite every second for revocation. Long-running queries are interrupted by dropping the in-flight proxy future and closing the client path; protocol-specific upstream cancellation is not complete yet.

Gatebase does not guarantee universal rollback. Rollback artifacts are not implemented yet; future rollback generation will be best-effort and only safe for constrained DML patterns.

MySQL proxy MVP requires client-side clear-password support toward Gatebase so the broker-issued session token can be verified. Native MySQL password-plugin token auth remains future work.
