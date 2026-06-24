import { useQuery } from "@tanstack/react-query";
import { RefreshCw } from "lucide-react";
import { api } from "../api";
import { Badge, Button, Card, Cell, Row, Table, fmtTime } from "../components/kit";
import { Query } from "../components/Query";
import { Pager, useServerPage } from "../components/Pager";

const PAGE_SIZE = 25;

export function Users() {
  const { page, setPage, limit, offset } = useServerPage(PAGE_SIZE);
  const query = useQuery({
    queryKey: ["users", page],
    queryFn: () => api.users({ limit, offset }),
  });
  const count = query.data?.length ?? 0;

  return (
    <Card
      title="Users"
      actions={
        <Button onClick={() => query.refetch()}>
          <RefreshCw />
          Refresh
        </Button>
      }
    >
      <Query query={query} empty={page > 0 ? "No more users." : "No users."}>
        {(rows) => (
          <Table columns={["ID", "Username", "Role", "Created", "Status"]}>
            {rows.map((u) => (
              <Row key={u.id}>
                <Cell mono>{u.id}</Cell>
                <Cell>{u.username}</Cell>
                <Cell>
                  <Badge tone={u.role === "admin" ? "brand" : "neutral"}>{u.role}</Badge>
                </Cell>
                <Cell>{fmtTime(u.created_at)}</Cell>
                <Cell>
                  {u.disabled_at ? (
                    <Badge tone="danger">disabled</Badge>
                  ) : (
                    <Badge tone="ok">active</Badge>
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
