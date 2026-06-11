import type { Metadata } from "next";
import type { ReactNode } from "react";

import { GlowBackground } from "@/components/site/glow-background";
import { SiteHeader } from "@/components/site/site-header";
import { SiteFooter } from "@/components/site/site-footer";

export const metadata: Metadata = {
  title: "Docs — codel00p",
  description:
    "How to install codel00p, run your first agent turn, and curate project memory.",
};

const sections = [
  { id: "overview", title: "Overview" },
  { id: "install", title: "Install" },
  { id: "quickstart", title: "Quickstart" },
  { id: "memory", title: "Project memory" },
  { id: "providers", title: "Providers" },
  { id: "mcp", title: "MCP tools" },
  { id: "commands", title: "Command reference" },
];

export default function DocsPage() {
  return (
    <div className="relative flex min-h-dvh flex-col">
      <GlowBackground />
      <SiteHeader />

      <main className="relative z-10 mx-auto w-full max-w-5xl flex-1 px-6 pb-24 pt-10">
        <div className="lg:grid lg:grid-cols-[180px_minmax(0,1fr)] lg:gap-14">
          {/* Table of contents */}
          <aside className="hidden lg:block">
            <nav className="sticky top-10">
              <p className="label mb-4 text-muted-foreground/70">Docs</p>
              <ul className="space-y-2.5">
                {sections.map((section) => (
                  <li key={section.id}>
                    <a
                      href={`#${section.id}`}
                      className="text-sm text-muted-foreground transition-colors hover:text-foreground"
                    >
                      {section.title}
                    </a>
                  </li>
                ))}
              </ul>
            </nav>
          </aside>

          {/* Content */}
          <article className="min-w-0 max-w-[68ch]">
            <p className="label text-brand/80">Documentation</p>
            <h1 className="mt-4 text-4xl font-medium tracking-tight text-foreground">
              Get started
            </h1>
            <p className="mt-4 text-lg leading-relaxed text-muted-foreground">
              codel00p is an open-source coding agent that turns finished work
              into reviewed, durable project memory. It runs locally from a
              single CLI.
            </p>

            <Section id="overview" title="Overview">
              <P>
                A run does real repository work through permissioned tools, then
                proposes <Em>memory candidates</Em> from what it learned. You
                review those candidates; only approved memory is fed back into
                future runs. Everything is stored in one local SQLite file you
                own.
              </P>
              <P>Three moving parts:</P>
              <Ul>
                <li>
                  <Strong>Harness</Strong> — reads, edits, runs commands,
                  inspects git, and resumes sessions inside a workspace.
                </li>
                <li>
                  <Strong>Memory</Strong> — a review lifecycle for project
                  knowledge: candidate, approved, archived.
                </li>
                <li>
                  <Strong>Providers</Strong> — a single contract over Anthropic,
                  OpenAI, Bedrock, Gemini, and OpenAI-compatible gateways.
                </li>
              </Ul>
            </Section>

            <Section id="install" title="Install">
              <P>
                codel00p builds from source. You need a recent Rust toolchain.
              </P>
              <Code>{`git clone https://github.com/inth3loop/codel00p
cd codel00p/core
cargo build --release --bin codel00p`}</Code>
              <P>
                The binary lands at <Ic>core/target/release/codel00p</Ic>. Put
                it on your <Ic>PATH</Ic> or call it directly.
              </P>
            </Section>

            <Section id="quickstart" title="Quickstart">
              <P>
                Set a provider key, then run a single agent turn. Every command
                is scoped to an organization and project, and writes to a local
                memory database.
              </P>
              <Code>{`export ANTHROPIC_API_KEY=sk-ant-...

codel00p \\
  --memory-db ./codel00p.db \\
  --organization-id acme \\
  --project-id web \\
  --project-name "Web App" \\
  agent run "Summarize how this project is built and tested" \\
  --provider anthropic \\
  --model claude-opus-4-8 \\
  --tool-set all`}</Code>
              <P>
                The four global flags before <Ic>agent</Ic> set the scope and
                are required on every command. <Ic>--tool-set</Ic> selects which
                tools the run may use: <Ic>read</Ic>, <Ic>edit</Ic>,{" "}
                <Ic>command</Ic>, <Ic>git</Ic>, or <Ic>all</Ic>. Add{" "}
                <Ic>--session-id</Ic> to persist and later resume a session.
              </P>
            </Section>

            <Section id="memory" title="Project memory">
              <P>
                A run proposes candidates; you decide what becomes durable. The
                examples below reuse the same scope flags from the quickstart.
              </P>
              <Code>{`# Review what the agent proposed
codel00p ...scope memory list

# Approve a candidate so future runs can use it
codel00p ...scope memory approve <memory-id>

# Search approved memory
codel00p ...scope memory search "deployment"`}</Code>
              <P>
                Only approved memory reaches the model on later runs. Candidates
                can also be edited, rejected, or archived, and every change is
                kept in an audit history you can inspect with{" "}
                <Ic>memory audit</Ic>. Add <Ic>--json</Ic> to most memory
                commands for machine-readable output.
              </P>
            </Section>

            <Section id="providers" title="Providers">
              <P>
                Pick a provider with <Ic>--provider</Ic> and a model with{" "}
                <Ic>--model</Ic>. Credentials are read from the environment.
              </P>
              <Code>{`anthropic     ANTHROPIC_API_KEY
openai        OPENAI_API_KEY
gemini        GEMINI_API_KEY  (or GOOGLE_API_KEY)
openrouter    OPENROUTER_API_KEY
bedrock       AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY
azure         AZURE_FOUNDRY_API_KEY`}</Code>
              <P>
                Point at an OpenAI-compatible gateway with <Ic>--base-url</Ic>,
                or apply an organization policy with{" "}
                <Ic>--provider-policy-preset</Ic>. Each variable also has a
                namespaced form, for example{" "}
                <Ic>CODEL00P_PROVIDER_OPENAI_API_KEY</Ic>, when you want keys
                scoped to codel00p alone.
              </P>
            </Section>

            <Section id="mcp" title="MCP tools">
              <P>
                codel00p speaks the Model Context Protocol in both directions.
                Attach an external MCP server to a run so the agent can use its
                tools:
              </P>
              <Code>{`codel00p ...scope agent run "..." \\
  --mcp-server files=mcp-server-filesystem \\
  --tool-set all`}</Code>
              <P>Or expose codel00p&apos;s own memory and sessions to another client:</P>
              <Code>{`codel00p ...scope mcp serve`}</Code>
              <P>
                Inspect attached servers with <Ic>agent mcp list</Ic> and{" "}
                <Ic>agent mcp doctor</Ic>.
              </P>
            </Section>

            <Section id="commands" title="Command reference">
              <P>Top-level commands, each under the global scope flags:</P>
              <Ul>
                <li>
                  <Ic>agent run</Ic> / <Ic>agent resume</Ic> — run or continue a
                  turn.
                </li>
                <li>
                  <Ic>memory</Ic> — list, search, show, approve, reject,
                  archive, edit, restore, audit, and quality review.
                </li>
                <li>
                  <Ic>session</Ic> — inspect persisted sessions and replay
                  events.
                </li>
                <li>
                  <Ic>mcp serve</Ic> — run codel00p as an MCP server.
                </li>
              </Ul>
              <P>
                Append <Ic>--help</Ic> to any command for its full options.
              </P>
            </Section>
          </article>
        </div>
      </main>

      <SiteFooter />
    </div>
  );
}

function Section({
  id,
  title,
  children,
}: {
  id: string;
  title: string;
  children: ReactNode;
}) {
  return (
    <section id={id} className="scroll-mt-10 border-t border-border/60 pt-12 mt-14">
      <h2 className="text-sm font-medium uppercase tracking-[0.18em] text-foreground">
        {title}
      </h2>
      <div className="mt-5">{children}</div>
    </section>
  );
}

function P({ children }: { children: ReactNode }) {
  return (
    <p className="mt-4 text-[0.95rem] leading-7 text-muted-foreground first:mt-0">
      {children}
    </p>
  );
}

function Ul({ children }: { children: ReactNode }) {
  return (
    <ul className="mt-4 space-y-2.5 text-[0.95rem] leading-7 text-muted-foreground marker:text-brand/60 [&>li]:pl-1 list-disc pl-5">
      {children}
    </ul>
  );
}

function Code({ children }: { children: string }) {
  return (
    <pre className="mt-5 overflow-x-auto rounded-xl border border-border bg-card/50 p-4 text-[0.82rem] leading-6">
      <code className="font-mono text-foreground/90">{children}</code>
    </pre>
  );
}

function Ic({ children }: { children: ReactNode }) {
  return (
    <code className="rounded-md border border-border/70 bg-card/60 px-1.5 py-0.5 font-mono text-[0.82em] text-foreground/90">
      {children}
    </code>
  );
}

function Em({ children }: { children: ReactNode }) {
  return <em className="text-foreground/90 not-italic">{children}</em>;
}

function Strong({ children }: { children: ReactNode }) {
  return <strong className="font-medium text-foreground">{children}</strong>;
}
