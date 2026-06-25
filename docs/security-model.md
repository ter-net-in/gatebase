# Security Model

Gatebase assumes:

- users satisfy configured broker access signals before receiving session tokens
- GitHub-specific access signals use GitHub issues and short-lived one-time tokens
- local CLI sessions are allowed only when target config sets `allow_cli_sessions: true`
- broker admin API callers authenticate with metadata-backed users and signed bearer tokens
- production database network access is restricted to Gatebase proxies
- audit sinks are protected from modification by normal users
- backups and PITR/WAL remain enabled for real recovery

Session enforcement happens at connection auth and during proxy activity. Proxies record active connections, close them when the session TTL expires, and poll the metadata store every second for revocation. Long-running queries are interrupted by dropping the in-flight proxy future and closing the client path; protocol-specific upstream cancellation is not complete yet.

Gatebase does not guarantee universal rollback. Rollback artifacts are best-effort
and only safe for constrained DML patterns; WAL/PITR remains the source of truth
for recovery.

MySQL proxy MVP requires client-side clear-password support toward Gatebase so the broker-issued session token can be verified. Native MySQL password-plugin token auth remains future work.

## Admin Users

The first admin user is bootstrapped locally with `gatebase admin user create --config ... --role admin --password-stdin`. After bootstrap, broker admin APIs require login through `/api/admin/login` and enforce roles. Admin bearer tokens are signed with `admin.signing_key_file`, separate from database session tokens. Each authenticated request re-checks the metadata-backed user, so deleted or disabled users lose access even if a previously issued token has not expired. User passwords are stored as Argon2 hashes in the metadata store.

Admin API roles are ordered `admin > operator > viewer`. `viewer` can read sessions and audit events, `operator` can also revoke sessions, and `admin` can manage users and run maintenance pruning.
