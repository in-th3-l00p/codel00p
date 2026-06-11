import type { Metadata } from "next";

import { DocHeader, H2, P, Ul, Ol, Ic, Strong } from "@/components/docs/prose";

export const metadata: Metadata = {
  title: "Project memory — codel00p docs",
  description:
    "The candidate, approved, and archived lifecycle for durable project knowledge.",
};

export default function MemoryPage() {
  return (
    <>
      <DocHeader
        group="Architecture"
        title="Project memory"
        lead="Memory is the differentiator. The goal is not to store every transcript, but to preserve useful knowledge in a compact, reviewed, reusable form."
      />

      <H2>What it captures</H2>
      <P>
        Memory entries are typed by kind so they stay organized and retrievable:
      </P>
      <Ul>
        <li>
          <Strong>Codebase facts</Strong> — stable facts about files, modules,
          and ownership;
        </li>
        <li>
          <Strong>Architecture decisions</Strong> — decisions, rationale, and
          tradeoffs;
        </li>
        <li>
          <Strong>Workflows</Strong> — setup, test, deploy, rollback, and
          debugging procedures;
        </li>
        <li>
          <Strong>Team conventions</Strong> — style, naming, and review
          preferences;
        </li>
        <li>
          <Strong>Task outcomes</Strong> and a <Strong>domain glossary</Strong>{" "}
          for product language.
        </li>
      </Ul>

      <H2>The lifecycle</H2>
      <P>
        Memory moves through a deliberate lifecycle. Review is the important
        step: unreviewed memory becomes noise quickly.
      </P>
      <Ol>
        <li>
          <Strong>Extract</Strong> — a run proposes candidates from useful work;
        </li>
        <li>
          <Strong>Review</Strong> — a human approves, edits, scopes, or rejects
          each candidate;
        </li>
        <li>
          <Strong>Store</Strong> — approved memory is saved to the project;
        </li>
        <li>
          <Strong>Retrieve</Strong> — future runs load relevant approved memory;
        </li>
        <li>
          <Strong>Refine</Strong> — stale or low-value memory is edited, merged,
          or archived.
        </li>
      </Ol>
      <P>
        A candidate is only ever a proposal. The model never sees a candidate on
        a later run, only entries you have explicitly approved.
      </P>

      <H2>Keeping memory trustworthy</H2>
      <P>
        Several deterministic signals help review stay efficient and keep the
        store clean:
      </P>
      <Ul>
        <li>
          <Strong>Duplicate detection</Strong> rejects exact duplicates and
          scores near-duplicates against existing active memory;
        </li>
        <li>
          <Strong>Stale detection</Strong> flags approved memory that newer
          entries appear to supersede;
        </li>
        <li>
          <Strong>Quality scoring</Strong> gives an advisory score and findings
          for content that is too short, too long, or vague;
        </li>
        <li>
          <Strong>Sensitivity scopes</Strong> mark entries normal or sensitive,
          and retrieval returns normal-only by default.
        </li>
      </Ul>
      <P>
        Every change to an entry is recorded in an <Strong>audit history</Strong>{" "}
        you can inspect with <Ic>memory audit</Ic>, and an edit can be rolled
        back with <Ic>memory restore</Ic>.
      </P>
    </>
  );
}
