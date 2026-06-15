import type { Metadata } from "next";

import { DocHeader, H2, P, Code, Ic, Ul, Note } from "@/components/docs/prose";
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

      <H2>Uninstall</H2>
      <P>
        The CLI removes itself. It shows what will be deleted and asks for
        confirmation first:
      </P>
      <Code>{`codel00p uninstall`}</Code>
      <P>By default it deletes the binary and keeps your data. Flags:</P>
      <Ul>
        <li>
          <Ic>--purge</Ic> — also delete <Ic>~/.codel00p</Ic> (config,
          credentials, saved sessions, and memory).
        </li>
        <li>
          <Ic>--yes</Ic> / <Ic>-y</Ic> — skip the prompt (required in
          non-interactive shells).
        </li>
      </Ul>
      <P>
        If you added the install directory to your shell <Ic>PATH</Ic>, remove
        that line too. On Windows the running binary cannot delete itself, so the
        command prints its path for manual removal.
      </P>
    </>
  );
}
