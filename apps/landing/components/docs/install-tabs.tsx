"use client";

import { useState, type ReactNode } from "react";
import { Check, Copy } from "lucide-react";

import { cn } from "@/lib/utils";

type Tab = {
  id: string;
  label: string;
  code: string;
  caption: ReactNode;
};

const tabs: Tab[] = [
  {
    id: "unix",
    label: "macOS & Linux",
    code: "curl -fsSL https://raw.githubusercontent.com/in-th3-l00p/codel00p/main/install.sh | sh",
    caption: (
      <>
        Installs to <code>~/.local/bin</code>. Set <code>CODEL00P_INSTALL_DIR</code>{" "}
        or <code>CODEL00P_VERSION</code> to customize.
      </>
    ),
  },
  {
    id: "windows",
    label: "Windows",
    code: "irm https://raw.githubusercontent.com/in-th3-l00p/codel00p/main/install.ps1 | iex",
    caption: <>Run in PowerShell. Adds codel00p to your user PATH.</>,
  },
  {
    id: "source",
    label: "From source",
    code: "git clone https://github.com/in-th3-l00p/codel00p\ncd codel00p/core\ncargo build --release --bin codel00p",
    caption: (
      <>
        Needs a recent Rust toolchain. The binary is self-contained, with SQLite
        compiled in.
      </>
    ),
  },
];

export function InstallTabs() {
  const [activeId, setActiveId] = useState(tabs[0].id);
  const [copied, setCopied] = useState(false);
  const active = tabs.find((tab) => tab.id === activeId) ?? tabs[0];

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(active.code);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // clipboard unavailable; ignore
    }
  };

  return (
    <div className="mt-6">
      <div className="overflow-hidden rounded-xl border border-border bg-card/40">
        {/* Tab bar */}
        <div className="flex items-center gap-2 border-b border-border/60 px-3 py-2.5">
          <span className="hidden gap-1.5 pr-1 sm:flex" aria-hidden>
            <span className="size-2.5 rounded-full bg-foreground/15" />
            <span className="size-2.5 rounded-full bg-foreground/15" />
            <span className="size-2.5 rounded-full bg-foreground/15" />
          </span>
          <div className="flex flex-1 flex-wrap gap-1">
            {tabs.map((tab) => {
              const isActive = tab.id === activeId;
              return (
                <button
                  key={tab.id}
                  type="button"
                  onClick={() => setActiveId(tab.id)}
                  aria-pressed={isActive}
                  className={cn(
                    "rounded-md px-2.5 py-1 font-mono text-xs transition-colors",
                    isActive
                      ? "bg-brand/15 text-foreground"
                      : "text-muted-foreground hover:text-foreground",
                  )}
                >
                  {tab.label}
                </button>
              );
            })}
          </div>
          <button
            type="button"
            onClick={copy}
            aria-label="Copy command"
            className="rounded-md p-1.5 text-muted-foreground transition-colors hover:bg-foreground/5 hover:text-foreground"
          >
            {copied ? (
              <Check className="size-4 text-brand" />
            ) : (
              <Copy className="size-4" />
            )}
          </button>
        </div>

        {/* Command */}
        <pre className="overflow-x-auto px-4 py-4 text-[0.82rem] leading-6">
          <code className="font-mono text-foreground/90">
            {active.code.split("\n").map((line, index) => (
              <span key={index} className="block">
                <span className="select-none text-brand/60">
                  {activeId === "windows" ? "> " : "$ "}
                </span>
                {line}
              </span>
            ))}
          </code>
        </pre>
      </div>

      <p className="mt-3 text-sm leading-6 text-muted-foreground [&_code]:rounded [&_code]:border [&_code]:border-border/70 [&_code]:bg-card/60 [&_code]:px-1 [&_code]:py-0.5 [&_code]:font-mono [&_code]:text-[0.82em] [&_code]:text-foreground/90">
        {active.caption}
      </p>
    </div>
  );
}
