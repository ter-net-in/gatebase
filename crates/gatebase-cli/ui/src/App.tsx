import { Tabs } from "@base-ui-components/react/tabs";
import { useQuery } from "@tanstack/react-query";
import { ShieldCheck } from "lucide-react";
import { api } from "./api";
import { Badge } from "./components/kit";
import { Sessions } from "./pages/Sessions";
import { Audits } from "./pages/Audits";
import { Users } from "./pages/Users";
import { Connections } from "./pages/Connections";
import { Logs } from "./pages/Logs";

const TABS = [
  { id: "sessions", label: "Sessions", el: <Sessions /> },
  { id: "audits", label: "Audits", el: <Audits /> },
  { id: "connections", label: "Connections", el: <Connections /> },
  { id: "users", label: "Users", el: <Users /> },
  { id: "logs", label: "Logs", el: <Logs /> },
] as const;

export function App() {
  const me = useQuery({ queryKey: ["me"], queryFn: () => api.me(), retry: 0 });

  return (
    <div className="mx-auto flex min-h-full max-w-6xl flex-col gap-6 px-6 py-8">
      <header className="flex items-center justify-between">
        <div className="flex items-center gap-2.5">
          <span className="grid size-9 place-items-center rounded-lg bg-primary text-primary-foreground">
            <ShieldCheck className="size-5" />
          </span>
          <div>
            <h1 className="text-lg font-semibold leading-tight tracking-tight">gatebase</h1>
            <p className="text-xs text-muted-foreground">database access console</p>
          </div>
        </div>
        {me.data && (
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <span>{me.data.username}</span>
            <Badge tone="brand">{me.data.role}</Badge>
          </div>
        )}
      </header>

      <Tabs.Root defaultValue="sessions">
        <Tabs.List className="inline-flex h-10 items-center justify-start gap-1 rounded-lg bg-muted p-1 text-muted-foreground">
          {TABS.map((t) => (
            <Tabs.Tab
              key={t.id}
              value={t.id}
              className="inline-flex cursor-pointer items-center justify-center whitespace-nowrap rounded-md px-3 py-1 text-sm font-medium transition-all hover:text-foreground focus-visible:outline-none data-[selected]:bg-background data-[selected]:text-foreground data-[selected]:shadow-sm"
            >
              {t.label}
            </Tabs.Tab>
          ))}
        </Tabs.List>
        {TABS.map((t) => (
          <Tabs.Panel key={t.id} value={t.id} className="mt-5 focus-visible:outline-none">
            {t.el}
          </Tabs.Panel>
        ))}
      </Tabs.Root>
    </div>
  );
}
