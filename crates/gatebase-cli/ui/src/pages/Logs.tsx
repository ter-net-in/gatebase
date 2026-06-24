import { useQuery } from "@tanstack/react-query";
import { api } from "../api";
import { Badge, Card, Cell, Row, Table, fmtTime } from "../components/kit";
import { Query } from "../components/Query";
import { Pager, useServerPage } from "../components/Pager";

const PAGE_SIZE = 25;

type Tone = "ok" | "warn" | "danger" | "brand" | "neutral";

const CATEGORIES: Record<string, { label: string; tone: Tone }> = {
  audit_allowed: { label: "query allowed", tone: "ok" },
  audit_blocked: { label: "query blocked", tone: "danger" },
  rollback: { label: "rollback captured", tone: "brand" },
  conn_opened: { label: "connection opened", tone: "neutral" },
  conn_closed: { label: "connection closed", tone: "neutral" },
};

// Server-side unified activity feed (audit + rollback + connection events),
// merged and paginated by the broker via GET /api/activity.
export function Logs() {
  const { page, setPage, limit, offset } = useServerPage(PAGE_SIZE);
  const query = useQuery({
    queryKey: ["activity", page],
    queryFn: () => api.activity({ limit, offset }),
  });
  const count = query.data?.length ?? 0;

  return (
    <Card title="Activity log">
      <Query query={query} empty={page > 0 ? "No more activity." : "No activity yet."}>
        {(rows) => (
          <Table columns={["Time", "Event", "Actor", "Target", "Detail"]}>
            {rows.map((e, i) => {
              const cat = CATEGORIES[e.category] ?? { label: e.category, tone: "neutral" as Tone };
              return (
                <Row key={`${e.time}-${i}`}>
                  <Cell>{fmtTime(e.time)}</Cell>
                  <Cell>
                    <Badge tone={cat.tone}>{cat.label}</Badge>
                  </Cell>
                  <Cell>{e.actor}</Cell>
                  <Cell>{e.target}</Cell>
                  <Cell mono>{e.detail}</Cell>
                </Row>
              );
            })}
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
