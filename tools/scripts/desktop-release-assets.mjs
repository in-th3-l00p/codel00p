#!/usr/bin/env node
import { createHash } from "node:crypto";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { basename, extname, join } from "node:path";
import { fileURLToPath } from "node:url";

export const desktopTargets = [
  {
    releaseTarget: "aarch64-apple-darwin",
    electronPlatform: "mac",
    electronArch: "arm64",
    extension: "dmg",
  },
  {
    releaseTarget: "x86_64-apple-darwin",
    electronPlatform: "mac",
    electronArch: "x64",
    extension: "dmg",
  },
  {
    releaseTarget: "x86_64-unknown-linux-gnu",
    electronPlatform: "linux",
    electronArch: "x64",
    extension: "AppImage",
  },
  {
    releaseTarget: "aarch64-unknown-linux-gnu",
    electronPlatform: "linux",
    electronArch: "arm64",
    extension: "AppImage",
  },
  {
    releaseTarget: "x86_64-pc-windows-msvc",
    electronPlatform: "win",
    electronArch: "x64",
    extension: "exe",
  },
];

export function desktopAssetName(target) {
  const releaseTarget =
    typeof target === "string" ? target : target.releaseTarget;
  const resolved = desktopTarget(releaseTarget);
  return `codel00p-desktop-${resolved.releaseTarget}.${resolved.extension}`;
}

export function normalizeDesktopArtifacts({
  sourceDir,
  outputDir,
  releaseTarget,
}) {
  const target = desktopTarget(releaseTarget);
  const sourceAsset = findDesktopArtifact(sourceDir, target.extension);
  const assetName = desktopAssetName(target);
  const outputAsset = join(outputDir, assetName);
  const outputChecksum = `${outputAsset}.sha256`;

  mkdirSync(outputDir, { recursive: true });
  copyFileSync(sourceAsset, outputAsset);
  writeFileSync(outputChecksum, `${sha256File(outputAsset)}  ${assetName}\n`);

  return [{ path: outputAsset }, { path: outputChecksum }];
}

export function findDesktopArtifact(sourceDir, extension) {
  const suffix = `.${extension}`;
  const candidates = walk(sourceDir)
    .filter((path) => extname(path) === suffix)
    .filter((path) => !path.includes("-unpacked"))
    .sort((a, b) => {
      const byTime = statSync(b).mtimeMs - statSync(a).mtimeMs;
      return byTime || b.localeCompare(a);
    });

  if (candidates.length === 0) {
    throw new Error(`no .${extension} desktop artifact found in ${sourceDir}`);
  }
  return candidates[0];
}

function desktopTarget(releaseTarget) {
  const target = desktopTargets.find(
    (candidate) => candidate.releaseTarget === releaseTarget,
  );
  if (!target) {
    throw new Error(`unsupported desktop release target: ${releaseTarget}`);
  }
  return target;
}

function walk(root) {
  if (!existsSync(root)) {
    throw new Error(`directory does not exist: ${root}`);
  }

  const paths = [];
  for (const entry of readdirSync(root)) {
    const path = join(root, entry);
    const stats = statSync(path);
    if (stats.isDirectory()) {
      paths.push(...walk(path));
    } else if (stats.isFile()) {
      paths.push(path);
    }
  }
  return paths;
}

function sha256File(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function usage() {
  return `Usage:
  node tools/scripts/desktop-release-assets.mjs manifest
  node tools/scripts/desktop-release-assets.mjs normalize --source <dir> --output <dir> --target <release-target>
`;
}

function option(args, name) {
  const index = args.indexOf(name);
  if (index === -1 || !args[index + 1]) {
    throw new Error(`missing ${name}`);
  }
  return args[index + 1];
}

function main() {
  const [command, ...args] = process.argv.slice(2);
  if (command === "manifest") {
    process.stdout.write(`${JSON.stringify(desktopTargets, null, 2)}\n`);
    return;
  }

  if (command === "normalize") {
    const artifacts = normalizeDesktopArtifacts({
      sourceDir: option(args, "--source"),
      outputDir: option(args, "--output"),
      releaseTarget: option(args, "--target"),
    });
    process.stdout.write(
      artifacts.map((artifact) => artifact.path).join("\n") + "\n",
    );
    return;
  }

  process.stderr.write(usage());
  process.exit(1);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main();
}
