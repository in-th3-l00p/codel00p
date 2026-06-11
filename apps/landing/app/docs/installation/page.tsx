import type { Metadata } from "next";

import { DocHeader, H2, P, Code, Ic, Note } from "@/components/docs/prose";

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
        lead="Install the prebuilt CLI with a single command. Building from source is optional."
      />

      <H2>macOS and Linux</H2>
      <P>
        The script downloads the right binary for your platform and installs it
        to <Ic>~/.local/bin</Ic>.
      </P>
      <Code>{`curl -fsSL https://raw.githubusercontent.com/in-th3-l00p/codel00p/main/install.sh | sh`}</Code>
      <P>
        Set <Ic>CODEL00P_INSTALL_DIR</Ic> to choose another location, or{" "}
        <Ic>CODEL00P_VERSION</Ic> to pin a specific release.
      </P>

      <H2>Windows</H2>
      <P>In PowerShell:</P>
      <Code>{`irm https://raw.githubusercontent.com/in-th3-l00p/codel00p/main/install.ps1 | iex`}</Code>

      <H2>Verify</H2>
      <Code>{`codel00p --help`}</Code>
      <P>
        You should see the top-level commands: <Ic>agent</Ic>, <Ic>memory</Ic>,{" "}
        <Ic>session</Ic>, and <Ic>mcp</Ic>. If the command is not found, add the
        install directory to your <Ic>PATH</Ic> as the installer prints.
      </P>

      <H2>Build from source</H2>
      <P>
        Prefer to build it yourself? You need a recent Rust toolchain. The binary
        is self-contained, with SQLite compiled in.
      </P>
      <Code>{`git clone https://github.com/in-th3-l00p/codel00p
cd codel00p/core
cargo build --release --bin codel00p`}</Code>
      <P>
        The binary lands at <Ic>core/target/release/codel00p</Ic>. Put it on your{" "}
        <Ic>PATH</Ic> or call it directly.
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
