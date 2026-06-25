# Completed: rollback parser support for `=` predicates and schema-qualified names

Status: implemented for Postgres and MySQL rollback parsers. The proxies now parse
schema-qualified quoted table names and `WHERE <pk> = <value>` predicates, and the
UI can download captured `before_rows` as CSV when artifacts contain rows.

## Context

Rollback capture is best-effort over a narrow SQL subset. A real production
delete did **not** get an auto-revert:

```sql
DELETE FROM "public"."cli_tokens" WHERE "id" = 'nua-VQWUKo6z_6Wj3Cmc4';
```

The recorded artifact (from `/opt/gatebase/data/rollback.jsonl`) shows why:

```json
{ "table": "public", "primary_key_column": null, "before_rows": [],
  "inverse_sql": null, "manual_required": true,
  "reason": "rollback requires WHERE <primary_key> IN (...)" }
```

Two parser gaps, both in `rollback.rs` of each proxy crate:

1. **Schema-qualified / quoted identifiers** — `take_ident` reads only the first
   quoted segment, so `"public"."cli_tokens"` parsed to `public`. The PK lookup
   and inverse SQL then target the wrong/again-unknown table.
2. **Single-equality predicate** — `parse_where_in` only matches `WHERE col IN
   (...)`. `WHERE "id" = '...'` falls through to a manual artifact before the PK
   is ever checked.

Goal: make `DELETE/UPDATE ... WHERE <pk> = <value>` and schema-qualified,
quoted table names auto-revertible (capture `before_rows`, emit inverse SQL),
matching the existing `IN (...)` behavior. PK-ness is already validated
downstream; the parser just needs to reach that point.

## Affected code

Two parallel implementations (Postgres uses `"`/`'`, MySQL uses `` ` ``/`'`):

- `crates/gatebase-proxy-postgres/src/rollback.rs`
- `crates/gatebase-proxy-mysql/src/rollback.rs`

Relevant functions (postgres line numbers; mysql mirrors):
- `parse_delete` (249), `parse_update` (270) — call the helpers below.
- `parse_where_in` (291) — only handles `IN (...)`.
- `take_ident` (317) — single-segment identifier reader.
- `split_table_name` / `fetch_single_primary_key` (≈175) — PK lookup; already
  splits `schema.table`, but must accept the qualified/quoted output of `take_ident`.
- `build_supported_artifact` (69), `build_insert_inverse`, `build_update_inverse`,
  `quote_ident` — downstream; should need no change once inputs are correct.

## Changes

### 1. Qualified, quoted identifiers — `take_ident`
Extend to read a dotted identifier where each segment may be quoted:
`"public"."cli_tokens"`, `public.cli_tokens`, `"cli_tokens"`, `cli_tokens`
(MySQL: backtick-quoted). Return a normalized table reference that
`split_table_name` can split into `(schema, table)` for the PK lookup, and that
`quote_ident` can re-quote for the generated `SELECT`/inverse SQL.

- Read segment 1 (quoted or bare); if the next char is `.`, read segment 2.
- Preserve the unquoted segment values; let `quote_ident`/`split_table_name`
  own re-quoting. Verify `split_table_name` handles a value that already lost its
  quotes (adjust if it currently expects raw quoted input).

### 2. Equality predicate — generalize `parse_where_in`
Rename to `parse_where_predicate` (keep a thin `IN` path) returning
`(column, Vec<String>)`:
- `WHERE <col> IN ( v1, v2, ... )` → existing behavior.
- `WHERE <col> = <value>` → `(col, vec![value])` (single-element list), so the
  rest of `build_supported_artifact` (PK match, `SELECT ... WHERE pk IN (value)`,
  inverse generation) works unchanged.
- Strip surrounding quotes on the column name as today; keep the value verbatim
  (it already carries its own quoting for the reconstructed `IN (...)`).
- Match `=` only as a top-level predicate (no `AND`/`OR`/`<`/`>`): if anything
  other than a single `col = value` (or `col IN (...)`) is present, fall back to
  the manual artifact (current conservative behavior). Do **not** attempt to
  parse compound WHERE clauses in this change.

Update `parse_delete` and `parse_update` to call `parse_where_predicate`.

### Safety / conservatism
- Keep the manual-artifact fallback for every unsupported shape — never emit an
  inverse we are not sure about.
- `max_rows` guard and the "WHERE column is not primary key" check are unchanged,
  so a non-PK equality still yields a manual artifact.

## Tests
Add unit tests in each `rollback.rs` (`#[cfg(test)]`) for the pure parser:
- `take_ident`: `"public"."cli_tokens"` → schema `public`, table `cli_tokens`;
  `public.users`; `"users"`; `users`; (mysql) backtick variants.
- `parse_where_predicate`: `WHERE "id" = 'x'` → `("id", ["'x'"])`;
  `WHERE id IN (1,2)` → `("id", ["1","2"])`; compound/`AND` → `None`.
- `parse_delete` / `parse_update` end-to-end on the `cli_tokens` statement →
  `RollbackRequest` with `table = public.cli_tokens`, `where_column = id`,
  one value (not manual).
- Regression: unsupported shapes still return the manual request.

## Verification
- Implemented with unit tests in both proxy crates.
- Verified with `cargo test -p gatebase-proxy-postgres -p gatebase-proxy-mysql`.
- Also covered by the current workspace `cargo clippy --workspace --all-targets`
  checks during release prep.

## Release / deploy
- Current workspace version is `0.4.5`.
- Releases build Linux/macOS tarballs named
  `gatebase-<version>-<target-triple>.tar.gz`.
- Servers can update with `gatebase update`, then restart the systemd services.
  No DB migration required (rollback artifact schema unchanged).

## Out of scope
- Compound `WHERE` clauses (`AND`/`OR`, ranges, `LIKE`).
- Multi-row inverse for statements without a single-column PK match.
- Composite primary keys (still manual, per `fetch_single_primary_key`).
