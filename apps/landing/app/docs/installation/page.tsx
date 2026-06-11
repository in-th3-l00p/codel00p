import type { Metadata } from "next";

import { DocHeader, H2, P, Code, Ic, Note } from "@/components/docs/prose";

export const metadata: Metadata = {
  title: "Installation — codel00p docs",
  description: "Build the codel00p CLI from source with a Rust toolchain.",
};

export default function InstallationPage() {
  return (
    <>
      <DocHeader
        group="Getting started"
        title="Installation"
        lead="codel00p builds from source. You need a recent Rust toolchain."
      />

      <H2>Build the CLI</H2>
      <Code>{`git clone https://github.com/inth3loop/codel00p
cd codel00p/core
cargo build --release --bin codel00p`}</Code>
      <P>
        The binary lands at <Ic>core/target/release/codel00p</Ic>. Put it on
        your <Ic>PATH</Ic> or call it directly.
      </P>

      <H2>Verify</H2>
      <Code>{`codel00p --help`}</Code>
      <P>
        You should see the top-level commands: <Ic>agent</Ic>, <Ic>memory</Ic>,{" "}
        <Ic>session</Ic>, and <Ic>mcp</Ic>.
      </P>

      <Note>
        <p>
          codel00p stores everything in a single SQLite file. Persisting memory
          and sessions to disk uses the storage backend&apos;s <Ic>sqlite</Ic>{" "}
          feature, which the release binary includes by default.
        </p>
      </Note>
    </>
  );
}
