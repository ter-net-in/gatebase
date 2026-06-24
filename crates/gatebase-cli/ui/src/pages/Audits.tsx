import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api, type AuditFilters } from "../api";
import { Badge, Button, Card, Cell, Row, Table, fmtTime } from "../components/kit";
import { Query } from "../components/Query";
import { Pager } from "../components/Pager";

const PAGE_SIZE = 50;
const input =
  "h-8 rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring";

const COLUMNS = ["Time", "Actor", "Target", "Engine", "Decision", "Rows", "Statement", "Rollback"];

function RollbackDetail({ auditId }: { auditId: string }) {
  const q = useQuery({
    queryKey: ["audit-rollback", auditId],
    queryFn: () => api.auditRollback(auditId),
  });
  return (
    <tr className="border-b bg-muted/30">
      <td colSpan={COLUMNS.length} className="px-6 py-4">
        {q.isLoading ? (
          <span className="text-sm text-muted-foreground">Loading rollback…</span>
        ) : q.isError ? (
          <span className="text-sm text-muted-foreground">
            Could not load rollback: {(q.error as Error).message}
          </span>
        ) : q.data ? (
          <div className="flex flex-col gap-2 text-sm">
            <div className="flex items-center gap-2">
              <span className="font-semibold">Revert for this statement</span>
              {q.data.manual_required ? (
                <Badge tone="warn">manual required</Badge>
              ) : (
                <Badge tone="ok">auto-revertible</Badge>
              )}
              {q.data.table_name && <Badge tone="neutral">{q.data.table_name}</Badge>}
            </div>
            <div>
              <div className="mb-1 text-xs uppercase tracking-wide text-muted-foreground">
                Inverse SQL
              </div>
              <pre className="overflow-x-auto rounded-lg border bg-background p-3 font-mono text-xs">
                {q.data.inverse_sql ?? "— (no automatic inverse; manual rollback needed)"}
              </pre>
            </div>
            {q.data.reason && (
              <div className="text-xs text-muted-foreground">Reason: {q.data.reason}</div>
            )}
          </div>
        ) : null}
      </td>
    </tr>
  );
}

export function Audits() {
  const [draft, setDraft] = useState<AuditFilters>({});
  const [filters, setFilters] = useState<AuditFilters>({});
  const [page, setPage] = useState(0);
  const [openId, setOpenId] = useState<string | null>(null);

  const query = useQuery({
    queryKey: ["audits", filters, page],
    queryFn: () =>
      api.audits({ ...filters, limit: PAGE_SIZE, offset: page * PAGE_SIZE }),
  });

  const apply = () => {
    setPage(0);
    setOpenId(null);
    setFilters({ ...draft });
  };
  const goto = (p: number) => {
    setOpenId(null);
    setPage(p);
  };

  const count = query.data?.length ?? 0;
  const hasNext = count === PAGE_SIZE;

  return (
    <Card
      title="Audit events"
      actions={
        <>
          <input
            className={input}
            placeholder="actor"
            value={draft.actor ?? ""}
            onChange={(e) => setDraft({ ...draft, actor: e.target.value || undefined })}
          />
          <input
            className={input}
            placeholder="target"
            value={draft.target ?? ""}
            onChange={(e) => setDraft({ ...draft, target: e.target.value || undefined })}
          />
          <select
            className={input}
            value={draft.decision ?? ""}
            onChange={(e) => setDraft({ ...draft, decision: e.target.value || undefined })}
          >
            <option value="">any decision</option>
            <option value="allowed">allowed</option>
            <option value="blocked">blocked</option>
          </select>
          <Button variant="brand" onClick={apply}>
            Apply
          </Button>
        </>
      }
    >
      <Query query={query} empty={page > 0 ? "No more events." : "No audit events match."}>
        {(rows) => (
          <Table columns={COLUMNS}>
            {rows.flatMap((e) => {
              const hasRollback = Boolean(e.rollback_artifact_id);
              const open = openId === e.id;
              const main = (
                <Row key={e.id}>
                  <Cell>{fmtTime(e.created_at)}</Cell>
                  <Cell>{e.actor}</Cell>
                  <Cell>{e.target}</Cell>
                  <Cell>{e.engine}</Cell>
                  <Cell>
                    {e.decision === "allowed" ? (
                      <Badge tone="ok">allowed</Badge>
                    ) : (
                      <Badge tone="danger">blocked</Badge>
                    )}
                  </Cell>
                  <Cell>{e.rows_affected ?? "—"}</Cell>
                  <Cell mono>
                    <span title={e.error ?? undefined}>{e.statement}</span>
                  </Cell>
                  <Cell>
                    {hasRollback ? (
                      <button
                        type="button"
                        className="rounded-md border border-input px-2 py-1 text-xs font-medium text-primary transition-colors hover:bg-accent"
                        onClick={() => setOpenId(open ? null : e.id)}
                      >
                        {open ? "hide revert" : "view revert"}
                      </button>
                    ) : (
                      <span className="text-xs text-muted-foreground">—</span>
                    )}
                  </Cell>
                </Row>
              );
              return open
                ? [main, <RollbackDetail key={`${e.id}-rb`} auditId={e.id} />]
                : [main];
            })}
          </Table>
        )}
      </Query>

      <Pager
        page={page}
        setPage={goto}
        hasNext={hasNext}
        label={`Page ${page + 1}${
          query.isFetching ? " · loading…" : count > 0 ? ` · ${count} rows` : ""
        }`}
      />
    </Card>
  );
}
