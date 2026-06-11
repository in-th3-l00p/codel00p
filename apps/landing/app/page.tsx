import Link from "next/link";
import { ArrowUpRight } from "lucide-react";

import { Button } from "@/components/ui/button";
import { GlowBackground } from "@/components/site/glow-background";
import { LoopMark } from "@/components/site/loop-mark";
import { SiteHeader } from "@/components/site/site-header";
import { SiteFooter } from "@/components/site/site-footer";

const REPO_URL = "https://github.com/inth3loop/codel00p";

const pillars = [
  {
    index: "01",
    title: "Durable memory",
    body: "Completed work becomes reviewed, searchable knowledge. Only approved memory ever reaches the model — nothing leaks in unread.",
  },
  {
    index: "02",
    title: "Real execution",
    body: "Read, edit, run, test, commit, resume. Workspace-safe tools with permissioned access and a deterministic audit trail.",
  },
  {
    index: "03",
    title: "Any provider",
    body: "Anthropic, OpenAI, Bedrock, Gemini, or your own gateway. One Rust contract with consistent policy, routing, and cost visibility.",
  },
];

export default function Home() {
  return (
    <div className="relative flex min-h-dvh flex-col">
      <GlowBackground />
      <SiteHeader />

      <main className="relative z-10 flex flex-1 flex-col">
        {/* Hero */}
        <section className="mx-auto flex w-full max-w-5xl flex-col items-center px-6 pb-20 pt-10 text-center sm:pb-28 sm:pt-24">
          <p
            className="label rise text-muted-foreground"
            style={{ animationDelay: "0ms" }}
          >
            Open source · agentic coding platform
          </p>

          <h1
            className="rise mt-3 px-2 pb-2 font-hand leading-[1.04] text-foreground sm:mt-4 sm:px-4"
            style={{
              animationDelay: "80ms",
              fontSize: "clamp(3.5rem, 17vw, 11.5rem)",
              textShadow:
                "0 0 90px color-mix(in oklab, var(--brand) 50%, transparent)",
            }}
          >
            codel00p
          </h1>

          <p
            className="rise mt-6 max-w-2xl text-balance text-2xl font-medium tracking-tight text-foreground sm:text-3xl"
            style={{ animationDelay: "160ms" }}
          >
            The coding agent that remembers.
          </p>

          <p
            className="rise mt-5 max-w-xl text-balance text-base leading-relaxed text-muted-foreground sm:text-lg"
            style={{ animationDelay: "220ms" }}
          >
            Most agents start every session from zero. codel00p turns real work
            into reviewed, durable project memory — so your codebase gets easier
            to work in every single time.
          </p>

          <div
            className="rise mt-10 flex flex-col items-center gap-3 sm:flex-row"
            style={{ animationDelay: "300ms" }}
          >
            <Button asChild className="h-11 rounded-full px-6 text-sm">
              <Link href="/docs">
                Read the docs
                <ArrowUpRight className="size-4" />
              </Link>
            </Button>
            <Button
              asChild
              variant="outline"
              className="h-11 rounded-full border-border bg-transparent px-6 text-sm text-foreground hover:bg-foreground/5"
            >
              <a href={REPO_URL} target="_blank" rel="noreferrer">
                View on GitHub
              </a>
            </Button>
          </div>

          <p
            className="rise label mt-12 text-muted-foreground/70"
            style={{ animationDelay: "380ms" }}
          >
            Rust core · provider-agnostic · local-first
          </p>
        </section>

        {/* Thesis */}
        <section className="mx-auto w-full max-w-3xl px-6 py-16 text-center sm:py-24">
          <p className="label text-brand/80">The thesis</p>
          <h2 className="mt-6 text-balance text-3xl font-medium tracking-tight text-foreground sm:text-[2.6rem] sm:leading-[1.1]">
            Model quality is rented.
            <br />
            <span className="text-muted-foreground">
              Project knowledge is owned.
            </span>
          </h2>
          <p className="mx-auto mt-7 max-w-xl text-balance text-base leading-relaxed text-muted-foreground sm:text-lg">
            The durable advantage isn&apos;t the model — it&apos;s the harness,
            the context, and the memory around it. codel00p makes that memory
            explicit: captured from real sessions, reviewed by your team, and
            reused automatically.
          </p>
        </section>

        {/* Pillars */}
        <section className="mx-auto w-full max-w-5xl px-6 py-12 sm:py-16">
          <div className="grid gap-px overflow-hidden rounded-2xl border border-border bg-border/60 sm:grid-cols-3">
            {pillars.map((pillar) => (
              <article
                key={pillar.index}
                className="group relative flex flex-col gap-4 bg-card/40 p-6 backdrop-blur-sm transition-colors hover:bg-card/70 sm:p-8"
              >
                <span className="font-mono text-xs text-brand/70">
                  {pillar.index}
                </span>
                <h3 className="text-lg font-medium tracking-tight text-foreground">
                  {pillar.title}
                </h3>
                <p className="text-sm leading-relaxed text-muted-foreground">
                  {pillar.body}
                </p>
              </article>
            ))}
          </div>
        </section>

        {/* Closing */}
        <section className="mx-auto flex w-full max-w-3xl flex-col items-center px-6 py-20 text-center sm:py-28">
          <LoopMark className="size-12" />
          <h2 className="mt-8 text-balance text-3xl font-medium tracking-tight text-foreground sm:text-4xl">
            Build a memory that{" "}
            <span
              className="font-hand text-[1.6em] leading-none text-brand"
              style={{
                textShadow:
                  "0 0 50px color-mix(in oklab, var(--brand) 55%, transparent)",
              }}
            >
              lasts
            </span>
            .
          </h2>
          <p className="mt-5 max-w-md text-balance text-base leading-relaxed text-muted-foreground">
            Free and open source. Run it locally today, bring it to your team
            tomorrow.
          </p>
          <div className="mt-9 flex flex-col items-center gap-3 sm:flex-row">
            <Button asChild className="h-11 rounded-full px-6 text-sm">
              <Link href="/docs">
                Get started
                <ArrowUpRight className="size-4" />
              </Link>
            </Button>
          </div>
        </section>
      </main>

      <SiteFooter />
    </div>
  );
}
