import type { ReactNode } from "react";

/** Page header: chapter eyebrow, title, and an optional lead paragraph. */
export function DocHeader({
  group,
  title,
  lead,
}: {
  group?: string | null;
  title: string;
  lead?: ReactNode;
}) {
  return (
    <header className="mb-10">
      {group ? <p className="label text-brand/80">{group}</p> : null}
      <h1 className="mt-4 text-4xl font-medium tracking-tight text-foreground">
        {title}
      </h1>
      {lead ? (
        <p className="mt-4 text-lg leading-relaxed text-muted-foreground">
          {lead}
        </p>
      ) : null}
    </header>
  );
}

export function H2({ children }: { children: ReactNode }) {
  return (
    <h2 className="mt-12 mb-4 text-xl font-medium tracking-tight text-foreground">
      {children}
    </h2>
  );
}

export function H3({ children }: { children: ReactNode }) {
  return (
    <h3 className="mt-8 mb-3 text-base font-medium tracking-tight text-foreground/90">
      {children}
    </h3>
  );
}

export function P({ children }: { children: ReactNode }) {
  return (
    <p className="mt-4 text-[0.95rem] leading-7 text-muted-foreground">
      {children}
    </p>
  );
}

export function Ul({ children }: { children: ReactNode }) {
  return (
    <ul className="mt-4 list-disc space-y-2.5 pl-5 text-[0.95rem] leading-7 text-muted-foreground marker:text-brand/60">
      {children}
    </ul>
  );
}

export function Ol({ children }: { children: ReactNode }) {
  return (
    <ol className="mt-4 list-decimal space-y-2.5 pl-5 text-[0.95rem] leading-7 text-muted-foreground marker:text-brand/60">
      {children}
    </ol>
  );
}

export function Code({ children }: { children: string }) {
  return (
    <pre className="mt-5 overflow-x-auto rounded-xl border border-border bg-card/50 p-4 text-[0.82rem] leading-6">
      <code className="font-mono text-foreground/90">{children}</code>
    </pre>
  );
}

/** ASCII diagram block — same shell as Code, centered, no horizontal scroll fuss. */
export function Diagram({ children }: { children: string }) {
  return (
    <pre className="mt-5 overflow-x-auto rounded-xl border border-border bg-card/30 p-5 text-[0.78rem] leading-5 text-muted-foreground">
      <code className="font-mono">{children}</code>
    </pre>
  );
}

export function Ic({ children }: { children: ReactNode }) {
  return (
    <code className="rounded-md border border-border/70 bg-card/60 px-1.5 py-0.5 font-mono text-[0.82em] text-foreground/90">
      {children}
    </code>
  );
}

export function Strong({ children }: { children: ReactNode }) {
  return <strong className="font-medium text-foreground">{children}</strong>;
}

export function Note({ children }: { children: ReactNode }) {
  return (
    <div className="mt-6 rounded-xl border border-border bg-card/40 p-4">
      <p className="label mb-2 text-brand/70">Note</p>
      <div className="text-[0.9rem] leading-7 text-muted-foreground [&>p]:mt-0">
        {children}
      </div>
    </div>
  );
}

export function Table({
  head,
  rows,
}: {
  head: string[];
  rows: ReactNode[][];
}) {
  return (
    <div className="mt-6 overflow-x-auto rounded-xl border border-border">
      <table className="w-full border-collapse text-left text-[0.88rem]">
        <thead>
          <tr className="border-b border-border bg-card/40">
            {head.map((cell) => (
              <th
                key={cell}
                className="px-4 py-2.5 font-medium text-foreground/90"
              >
                {cell}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row, r) => (
            <tr key={r} className="border-b border-border/50 last:border-0">
              {row.map((cell, c) => (
                <td
                  key={c}
                  className="px-4 py-2.5 align-top text-muted-foreground [&_code]:text-foreground/90"
                >
                  {cell}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
