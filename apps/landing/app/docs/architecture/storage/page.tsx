import type { Metadata } from "next";

import { DocHeader, H2, P, Ul, Ic, Strong } from "@/components/docs/prose";

export const metadata: Metadata = {
  title: "Storage & sessions — codel00p docs",
  description:
    "The persistence boundary, scoping, and how durable sessions are stored and replayed.",
};

export default function StoragePage() {
  return (
    <>
      <DocHeader
        group="Architecture"
        title="Storage & sessions"
        lead="codel00p-storage is the one place that talks to a backend. Every other crate persists data through backend-neutral traits, so the backend stays replaceable."
      />

      <H2>Primitives</H2>
      <P>The storage API is intentionally small and capability-based:</P>
      <Ul>
        <li>
          <Ic>KeyValueStore</Ic> — scoped settings, cursors, and lightweight
          runtime state;
        </li>
        <li>
          <Ic>DocumentStore</Ic> — structured records by collection and id;
        </li>
        <li>
          <Ic>AppendLogStore</Ic> — ordered streams for sessions, memory
          evolution, and audit history.
        </li>
      </Ul>
      <P>
        It is not an ORM and does not leak SQL concepts to the crates above it.
      </P>

      <H2>Scope</H2>
      <P>
        Every primitive is keyed by a <Ic>StorageScope</Ic>, which can represent
        global state, a workspace, a user, or an organization and project pair.
        That is why every CLI command takes <Ic>--organization-id</Ic> and{" "}
        <Ic>--project-id</Ic>: they select the scope your data is stored under.
        Backends map a scope to tables, key prefixes, or tenant ids, but callers
        see one stable model.
      </P>

      <H2>Backends</H2>
      <P>Two backends implement the same traits today:</P>
      <Ul>
        <li>
          <Strong>In-memory</Strong> for tests and harness development;
        </li>
        <li>
          <Strong>SQLite</Strong> for durable local project state, which is what
          the CLI uses through <Ic>--memory-db</Ic>.
        </li>
      </Ul>
      <P>
        Redis and cloud backends are planned behind the same traits, so domain
        crates never change when the backend does.
      </P>

      <H2>Durable sessions</H2>
      <P>
        <Ic>codel00p-session</Ic> builds on those primitives: it stores session
        metadata as documents and the transcript as an append log. Because the
        harness emits a typed event for every step, a stored session can be{" "}
        <Strong>replayed</Strong> exactly, which is what <Ic>agent resume</Ic>{" "}
        and the <Ic>session</Ic> commands rely on. Approved memory is stored the
        same way, with its review and audit history kept as append logs.
      </P>
    </>
  );
}
