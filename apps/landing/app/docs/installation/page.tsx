import type { Metadata } from "next";

import { DocHeader, H2, P, Code, Ic, Note } from "@/components/docs/prose";
import { InstallTabs } from "@/components/docs/install-tabs";

export const metadata: Metadata = {
  title: "Installation — codel00p docs",
  description:
    "Install the prebuilt codel00p CLI with one command, or build from source.",
};

export default function InstallationPage() {
  return (
    <>
      <DocHeader
        group="Getting started"
        title="Installation"
        lead="Install the prebuilt CLI with a single command. Pick your platform below."
      />

      <InstallTabs />

      <H2>Verify</H2>
      <Code>{`codel00p --help`}</Code>
      <P>
        You should see the top-level commands: <Ic>agent</Ic>, <Ic>memory</Ic>,{" "}
        <Ic>session</Ic>, and <Ic>mcp</Ic>. If the command is not found, add the
        install directory to your <Ic>PATH</Ic> as the installer prints.
      </P>

      <Note>
        <p>
          Prebuilt binaries are published for macOS (Apple silicon and Intel),
          Linux (x86-64 and arm64), and Windows (x86-64) on each tagged release.
        </p>
      </Note>
    </>
  );
}
