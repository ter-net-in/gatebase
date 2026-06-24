// Types mirror the broker DTOs (crates/gatebase-broker/src/dto.rs).
// All requests go to the same-origin local `gatebase ui` proxy, which injects
// the bearer token — the browser never holds the admin token.

export interface Session {
  session_id: string;
  actor: string;
  github_repo: string;
  issue: number | null;
  target: string;
  expires_at: string;
  revoked_at: string | null;
}

export interface AuditEvent {
  id: string;
  session_id: string;
  actor: string;
  target: string;
  engine: string;
  statement: string;
  decision: string;
  rows_affected: number | null;
  error: string | null;
  created_at: string;
  rollback_artifact_id: string | null;
}

export interface User {
  id: string;
  username: string;
  role: string;
  created_at: string;
  disabled_at: string | null;
}

export interface Rollback {
  id: string;
  session_id: string;
  actor: string;
  target: string;
  engine: string;
  statement: string;
  table_name: string | null;
  primary_key_column: string | null;
  inverse_sql: string | null;
  manual_required: boolean;
  reason: string | null;
  created_at: string;
}

export interface Connection {
  id: string;
  session_id: string;
  target: string;
  client_addr: string;
  connected_at: string;
  disconnected_at: string | null;
}

export interface Me {
  username: string;
  role: string;
}

export interface Activity {
  time: string;
  category: string;
  actor: string;
  target: string;
  detail: string;
}

export interface AuditFilters {
  actor?: string;
  target?: string;
  decision?: string;
  limit?: number;
  offset?: number;
}

export interface Page {
  limit?: number;
  offset?: number;
}

function pageQs(p: Page = {}): string {
  const params = new URLSearchParams();
  if (p.limit) params.set("limit", String(p.limit));
  if (p.offset) params.set("offset", String(p.offset));
  const s = params.toString();
  return s ? `?${s}` : "";
}

async function get<T>(path: string): Promise<T> {
  const res = await fetch(path, { headers: { accept: "application/json" } });
  if (!res.ok) {
    const body = await res.text();
    throw new Error(`${res.status}: ${body || res.statusText}`);
  }
  return res.json() as Promise<T>;
}

function qs(filters: AuditFilters): string {
  const params = new URLSearchParams();
  if (filters.actor) params.set("actor", filters.actor);
  if (filters.target) params.set("target", filters.target);
  if (filters.decision) params.set("decision", filters.decision);
  if (filters.limit) params.set("limit", String(filters.limit));
  if (filters.offset) params.set("offset", String(filters.offset));
  const s = params.toString();
  return s ? `?${s}` : "";
}

export const api = {
  me: () => get<Me>("/api/admin/me"),
  sessions: (page: Page = {}) => get<Session[]>(`/api/sessions${pageQs(page)}`),
  audits: (filters: AuditFilters = {}) =>
    get<AuditEvent[]>(`/api/audit/events${qs(filters)}`),
  users: (page: Page = {}) => get<User[]>(`/api/admin/users${pageQs(page)}`),
  rollbacks: (page: Page = {}) => get<Rollback[]>(`/api/rollbacks${pageQs(page)}`),
  connections: (page: Page = {}) =>
    get<Connection[]>(`/api/connections${pageQs(page)}`),
  activity: (page: Page = {}) => get<Activity[]>(`/api/activity${pageQs(page)}`),
  auditRollback: (id: string) =>
    get<Rollback>(`/api/audit/events/${encodeURIComponent(id)}/rollback`),
};
