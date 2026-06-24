import type { ReactNode } from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "../lib/utils";

// shadcn-style primitives layered over the Tailwind theme. Same component API
// as before so pages need no changes.

export function Card({
  title,
  actions,
  children,
}: {
  title?: ReactNode;
  actions?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="rounded-xl border bg-card text-card-foreground shadow-sm">
      {(title || actions) && (
        <header className="flex flex-wrap items-center justify-between gap-3 border-b px-6 py-4">
          <h2 className="text-base font-semibold leading-none tracking-tight">{title}</h2>
          <div className="flex flex-wrap items-center gap-2">{actions}</div>
        </header>
      )}
      <div>{children}</div>
    </section>
  );
}

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-1.5 whitespace-nowrap rounded-md text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background disabled:pointer-events-none disabled:opacity-40 [&_svg]:size-4 [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        brand: "bg-primary text-primary-foreground shadow-sm hover:bg-primary/90",
        default: "border border-input bg-transparent shadow-sm hover:bg-accent hover:text-accent-foreground",
        ghost: "hover:bg-accent hover:text-accent-foreground",
        destructive: "bg-destructive text-destructive-foreground shadow-sm hover:bg-destructive/90",
      },
      size: {
        default: "h-9 px-4 py-2",
        sm: "h-8 rounded-md px-3 text-xs",
      },
    },
    defaultVariants: { variant: "default", size: "sm" },
  },
);

export function Button({
  children,
  onClick,
  variant,
  size,
  disabled = false,
}: {
  children: ReactNode;
  onClick?: () => void;
  disabled?: boolean;
} & VariantProps<typeof buttonVariants>) {
  return (
    <button
      type="button"
      className={cn(buttonVariants({ variant, size }))}
      onClick={onClick}
      disabled={disabled}
    >
      {children}
    </button>
  );
}

type Tone = "neutral" | "ok" | "warn" | "danger" | "brand";

const badgeTones: Record<Tone, string> = {
  neutral: "border-transparent bg-secondary text-secondary-foreground",
  ok: "border-success/30 bg-success/15 text-success",
  warn: "border-warning/30 bg-warning/15 text-warning",
  danger: "border-destructive/30 bg-destructive/15 text-destructive",
  brand: "border-primary/30 bg-primary/15 text-primary",
};

export function Badge({ children, tone = "neutral" }: { children: ReactNode; tone?: Tone }) {
  return (
    <span
      className={cn(
        "inline-flex items-center rounded-md border px-2 py-0.5 text-xs font-medium",
        badgeTones[tone],
      )}
    >
      {children}
    </span>
  );
}

export function Table({ columns, children }: { columns: string[]; children: ReactNode }) {
  return (
    <div className="relative w-full overflow-x-auto">
      <table className="w-full caption-bottom text-sm">
        <thead className="[&_tr]:border-b">
          <tr className="border-b transition-colors">
            {columns.map((c) => (
              <th
                key={c}
                className="h-10 px-4 text-left align-middle text-xs font-medium uppercase tracking-wide text-muted-foreground"
              >
                {c}
              </th>
            ))}
          </tr>
        </thead>
        <tbody className="[&_tr:last-child]:border-0">{children}</tbody>
      </table>
    </div>
  );
}

export function Row({ children }: { children: ReactNode }) {
  return (
    <tr className="border-b align-top transition-colors hover:bg-muted/50">{children}</tr>
  );
}

export function Cell({ children, mono }: { children: ReactNode; mono?: boolean }) {
  return (
    <td className={cn("p-4 align-middle", mono && "font-mono text-xs")}>{children}</td>
  );
}

export function StateNote({ children }: { children: ReactNode }) {
  return <div className="px-4 py-10 text-center text-sm text-muted-foreground">{children}</div>;
}

export function fmtTime(value: string | null): string {
  if (!value) return "—";
  const d = new Date(value);
  if (Number.isNaN(d.getTime())) return value;
  return d.toLocaleString();
}
