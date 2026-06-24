import { useState } from "react";
import type { ReactNode } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { Button } from "./kit";

// Client-side pagination over an in-memory array.
export function usePaged<T>(items: T[] | undefined, size: number) {
  const [page, setPage] = useState(0);
  const total = items?.length ?? 0;
  const pageCount = Math.max(1, Math.ceil(total / size));
  const clamped = Math.min(page, pageCount - 1);
  const slice = (items ?? []).slice(clamped * size, clamped * size + size);
  return {
    page: clamped,
    setPage,
    slice,
    total,
    hasNext: (clamped + 1) * size < total,
  };
}

// Server-side pagination state: tracks page and derives limit/offset to send.
export function useServerPage(pageSize: number) {
  const [page, setPage] = useState(0);
  return { page, setPage, limit: pageSize, offset: page * pageSize };
}

export function Pager({
  page,
  setPage,
  hasNext,
  label,
}: {
  page: number;
  setPage: (n: number) => void;
  hasNext: boolean;
  label?: ReactNode;
}) {
  return (
    <footer className="flex items-center justify-between gap-3 border-t px-6 py-3 text-sm text-muted-foreground">
      <span>{label ?? `Page ${page + 1}`}</span>
      <div className="flex items-center gap-2">
        <Button disabled={page === 0} onClick={() => setPage(Math.max(0, page - 1))}>
          <ChevronLeft />
          Prev
        </Button>
        <Button disabled={!hasNext} onClick={() => setPage(page + 1)}>
          Next
          <ChevronRight />
        </Button>
      </div>
    </footer>
  );
}
