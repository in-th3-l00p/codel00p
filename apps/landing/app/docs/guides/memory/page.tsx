import type { Metadata } from "next";

import { DocHeader, H2, P, Code, Ic, Note } from "@/components/docs/prose";

export const metadata: Metadata = {
  title: "Curating memory — codel00p docs",
  description:
    "Review, approve, search, edit, and prune project memory from the CLI.",
};

export default function MemoryGuidePage() {
  return (
    <>
      <DocHeader
        group="Guides"
        title="Curating memory"
        lead="The day-to-day workflow for turning candidates into trusted, durable knowledge. Each command takes the same scope flags from the quick start."
      />

      <H2>Review and approve</H2>
      <P>
        List what runs have proposed, inspect a candidate, then approve, reject,
        or archive it.
      </P>
      <Code>{`codel00p ...scope memory list
codel00p ...scope memory show <memory-id>
codel00p ...scope memory approve <memory-id>
codel00p ...scope memory reject <memory-id>
codel00p ...scope memory archive <memory-id>`}</Code>

      <H2>Search and edit</H2>
      <P>
        Search approved memory by text, and edit an entry in place. An edit
        preserves status, source, and tags, and is fully reversible.
      </P>
      <Code>{`codel00p ...scope memory search "deployment"
codel00p ...scope memory edit <memory-id> "Updated content"
codel00p ...scope memory restore <memory-id> <audit-sequence>
codel00p ...scope memory audit <memory-id>`}</Code>

      <H2>Keep the store healthy</H2>
      <P>
        Three review queries surface entries worth attention: likely duplicates,
        memory that newer entries supersede, and low-quality content.
      </P>
      <Code>{`codel00p ...scope memory similar
codel00p ...scope memory stale
codel00p ...scope memory quality`}</Code>

      <Note>
        <p>
          Add <Ic>--json</Ic> to most memory commands for machine-readable
          output, and <Ic>--sensitivity sensitive</Ic> to include entries marked
          sensitive, which are hidden from retrieval by default.
        </p>
      </Note>
    </>
  );
}
