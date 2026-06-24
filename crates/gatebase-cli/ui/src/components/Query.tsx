import type { UseQueryResult } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { StateNote } from "./kit";

// Renders the standard loading / error / empty states for a query, delegating
// to `children` once data is present.
export function Query<T>({
  query,
  empty = "Nothing here yet.",
  children,
}: {
  query: UseQueryResult<T[]>;
  empty?: string;
  children: (data: T[]) => ReactNode;
}) {
  if (query.isLoading) return <StateNote>Loading…</StateNote>;
  if (query.isError)
    return <StateNote>Failed to load: {(query.error as Error).message}</StateNote>;
  const data = query.data ?? [];
  if (data.length === 0) return <StateNote>{empty}</StateNote>;
  return <>{children(data)}</>;
}
