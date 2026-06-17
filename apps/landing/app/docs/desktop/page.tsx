import type { Metadata } from "next";

import {
  Code,
  DocHeader,
  H2,
  Ic,
  Note,
  P,
  Table,
  Ul,
} from "@/components/docs/prose";

const RELEASE_DOWNLOAD_BASE =
  "https://github.com/in-th3-l00p/codel00p/releases/latest/download";

const desktopAssets = [
  {
    platform: "macOS Apple silicon",
    asset: "codel00p-desktop-aarch64-apple-darwin.dmg",
    notes: "For M-series Macs.",
  },
  {
    platform: "macOS Intel",
    asset: "codel00p-desktop-x86_64-apple-darwin.dmg",
    notes: "For Intel Macs.",
  },
  {
    platform: "Linux x86-64",
    asset: "codel00p-desktop-x86_64-unknown-linux-gnu.AppImage",
    notes: "Mark executable before launching.",
  },
  {
    platform: "Linux arm64",
    asset: "codel00p-desktop-aarch64-unknown-linux-gnu.AppImage",
    notes: "For arm64 Linux desktops.",
  },
  {
    platform: "Windows x86-64",
    asset: "codel00p-desktop-x86_64-pc-windows-msvc.exe",
    notes: "Runs the NSIS installer.",
  },
];

export const metadata: Metadata = {
  title: "Desktop app — codel00p docs",
  description:
    "Download the codel00p desktop app release builds and understand how they are published.",
};

export default function DesktopPage() {
  return (
    <>
      <DocHeader
        group="Getting started"
        title="Desktop app"
        lead="Download the Electron control center from the same tagged GitHub Releases that publish the CLI."
      />

      <P>
        The desktop app is a thin Electron shell over the shared codel00p
        protocols. It signs in through the browser, reads cloud dashboard data
        through <Ic>@codel00p/sdk</Ic>, and connects to local sessions through
        the installed <Ic>codel00p</Ic> CLI.
      </P>

      <H2>Downloads</H2>
      <Table
        head={["Platform", "Asset", "Notes"]}
        rows={desktopAssets.map((asset) => [
          asset.platform,
          <a
            key={asset.asset}
            href={`${RELEASE_DOWNLOAD_BASE}/${asset.asset}`}
            className="font-mono text-[0.82rem] text-foreground underline decoration-brand/50 underline-offset-4 hover:decoration-brand"
          >
            {asset.asset}
          </a>,
          asset.notes,
        ])}
      />

      <Note>
        <p>
          Each desktop asset also has a matching <Ic>.sha256</Ic> sidecar in the
          GitHub Release. macOS builds are unsigned for now, so Gatekeeper may
          require an explicit open from Finder.
        </p>
      </Note>

      <H2>Linux launch</H2>
      <Code>{`chmod +x codel00p-desktop-x86_64-unknown-linux-gnu.AppImage
./codel00p-desktop-x86_64-unknown-linux-gnu.AppImage`}</Code>

      <H2>Release process</H2>
      <P>
        Desktop installers are built by <Ic>.github/workflows/release.yml</Ic>{" "}
        when a <Ic>v*</Ic> tag is pushed. The workflow runs Electron Builder for
        each supported platform, then normalizes the output to the stable asset
        names above before the final GitHub Release publish step.
      </P>
      <Ul>
        <li>
          Bump <Ic>apps/desktop/package.json</Ic> when the desktop app changes.
        </li>
        <li>
          Commit the version change and push a <Ic>vX.Y.Z</Ic> tag, matching the
          CLI release convention.
        </li>
        <li>
          The release workflow uploads both the installer and its{" "}
          <Ic>.sha256</Ic> checksum sidecar.
        </li>
      </Ul>
    </>
  );
}
