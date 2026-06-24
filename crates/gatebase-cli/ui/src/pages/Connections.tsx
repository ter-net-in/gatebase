import { useQuery } from "@tanstack/react-query";
import { RefreshCw } from "lucide-react";
import { api } from "../api";
import { Badge, Button, Card, Cell, Row, Table, fmtTime } from "../components/kit";
import { Query } from "../components/Query";
import { Pager, useServerPage } from "../components/Pager";

const PAGE_SIZE = 25;

export function Connections() {
  const { page, setPage, limit, offset } = useServerPage(PAGE_SIZE);
  const query = useQuery({
    queryKey: ["connections", page],
    queryFn: () => api.connections({ limit, offset }),
  });
  const count = query.data?.length ?? 0;

  return (
    <Card
      title="Active connections"
      actions={
        <Button onClick={() => query.refetch()}>
          <RefreshCw />
          Refresh
        </Button>
      }
    >
      <Query query={query} empty={page > 0 ? "No more connections." : "No active connections."}>
        {(rows) => (
          <Table columns={["Connection", "Session", "Target", "Client", "Connected", "Status"]}>
            {rows.map((c) => (
              <Row key={c.id}>
                <Cell mono>{c.id}</Cell>
                <Cell mono>{c.session_id}</Cell>
                <Cell>{c.target}</Cell>
                <Cell mono>{c.client_addr}</Cell>
                <Cell>{fmtTime(c.connected_at)}</Cell>
                <Cell>
                  {c.disconnected_at ? (
                    <Badge tone="neutral">closed</Badge>
                  ) : (
                    <Badge tone="ok">live</Badge>
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
