import { useQuery } from "@tanstack/react-query";
import { RefreshCw } from "lucide-react";
import { api, type Session } from "../api";
import { Badge, Button, Card, Cell, Row, Table, fmtTime } from "../components/kit";
import { Query } from "../components/Query";
import { Pager, useServerPage } from "../components/Pager";

const PAGE_SIZE = 25;

function isActive(s: Session): boolean {
  if (s.revoked_at) return false;
  const exp = new Date(s.expires_at).getTime();
  return Number.isNaN(exp) ? false : exp > Date.now();
}

export function Sessions() {
  const { page, setPage, limit, offset } = useServerPage(PAGE_SIZE);
  const query = useQuery({
    queryKey: ["sessions", page],
    queryFn: () => api.sessions({ limit, offset }),
  });
  const count = query.data?.length ?? 0;

  return (
    <Card
      title="Sessions"
      actions={
        <Button onClick={() => query.refetch()}>
          <RefreshCw />
          Refresh
        </Button>
      }
    >
      <Query query={query} empty={page > 0 ? "No more sessions." : "No sessions."}>
        {(rows) => (
          <Table columns={["Session", "Actor", "Repo", "Issue", "Target", "Expires", "Status"]}>
            {rows.map((s) => (
              <Row key={s.session_id}>
                <Cell mono>{s.session_id}</Cell>
                <Cell>{s.actor}</Cell>
                <Cell>{s.github_repo || "—"}</Cell>
                <Cell>{s.issue ?? "—"}</Cell>
                <Cell>{s.target}</Cell>
                <Cell>{fmtTime(s.expires_at)}</Cell>
                <Cell>
                  {isActive(s) ? (
                    <Badge tone="ok">active</Badge>
                  ) : (
                    <Badge tone="neutral">{s.revoked_at ? "revoked" : "expired"}</Badge>
                  )}
                </Cell>
              </Row>
            ))}
          </Table>
        )}
      </Query>
      <Pager
        page={page}
        setPage={setPage}
        hasNext={count === PAGE_SIZE}
        label={`Page ${page + 1}${query.isFetching ? " · loading…" : ` · ${count} rows`}`}
      />
    </Card>
  );
}
