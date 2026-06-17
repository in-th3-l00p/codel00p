import assert from "node:assert/strict";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  utimesSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import { test } from "node:test";

import {
  desktopAssetName,
  desktopTargets,
  normalizeDesktopArtifacts,
} from "./desktop-release-assets.mjs";

test("desktop release targets use the same target-triple naming style as CLI assets", () => {
  assert.deepEqual(
    desktopTargets.map((target) => target.releaseTarget),
    [
      "aarch64-apple-darwin",
      "x86_64-apple-darwin",
      "x86_64-unknown-linux-gnu",
      "aarch64-unknown-linux-gnu",
      "x86_64-pc-windows-msvc",
    ],
  );

  assert.deepEqual(
    desktopTargets.map((target) => desktopAssetName(target)),
    [
      "codel00p-desktop-aarch64-apple-darwin.dmg",
      "codel00p-desktop-x86_64-apple-darwin.dmg",
      "codel00p-desktop-x86_64-unknown-linux-gnu.AppImage",
      "codel00p-desktop-aarch64-unknown-linux-gnu.AppImage",
      "codel00p-desktop-x86_64-pc-windows-msvc.exe",
    ],
  );
});

test("normalizes the packaged desktop installer and writes a sha256 sidecar", () => {
  const root = mkdtempSync(join(tmpdir(), "codel00p-desktop-release-"));
  const sourceDir = join(root, "dist");
  const outputDir = join(root, "release");
  const sourceAsset = join(sourceDir, "codel00p Setup 0.1.0.exe");
  mkdirSync(sourceDir, { recursive: true });
  writeFileSync(sourceAsset, "desktop installer bytes");
  writeFileSync(join(sourceDir, "latest.yml"), "ignored updater metadata");

  const artifacts = normalizeDesktopArtifacts({
    sourceDir,
    outputDir,
    releaseTarget: "x86_64-pc-windows-msvc",
  });

  assert.deepEqual(
    artifacts.map((artifact) => basename(artifact.path)),
    [
      "codel00p-desktop-x86_64-pc-windows-msvc.exe",
      "codel00p-desktop-x86_64-pc-windows-msvc.exe.sha256",
    ],
  );
  assert.ok(existsSync(join(outputDir, "codel00p-desktop-x86_64-pc-windows-msvc.exe")));
  assert.match(
    readFileSync(
      join(outputDir, "codel00p-desktop-x86_64-pc-windows-msvc.exe.sha256"),
      "utf8",
    ),
    /^[a-f0-9]{64}  codel00p-desktop-x86_64-pc-windows-msvc\.exe\n$/,
  );
});

test("normalizes the newest matching desktop artifact when old local builds remain", () => {
  const root = mkdtempSync(join(tmpdir(), "codel00p-desktop-release-"));
  const sourceDir = join(root, "dist");
  const outputDir = join(root, "release");
  const oldAsset = join(sourceDir, "codel00p-desktop-0.1.0-mac-arm64.dmg");
  const newAsset = join(sourceDir, "codel00p-desktop-0.4.0-mac-arm64.dmg");
  mkdirSync(sourceDir, { recursive: true });
  writeFileSync(oldAsset, "old desktop installer bytes");
  writeFileSync(newAsset, "new desktop installer bytes");
  utimesSync(oldAsset, new Date("2026-01-01T00:00:00Z"), new Date("2026-01-01T00:00:00Z"));
  utimesSync(newAsset, new Date("2026-01-02T00:00:00Z"), new Date("2026-01-02T00:00:00Z"));

  normalizeDesktopArtifacts({
    sourceDir,
    outputDir,
    releaseTarget: "aarch64-apple-darwin",
  });

  assert.equal(
    readFileSync(join(outputDir, "codel00p-desktop-aarch64-apple-darwin.dmg"), "utf8"),
    "new desktop installer bytes",
  );
});

test("desktop electron-builder config uses a filesystem-safe executable name", () => {
  const desktopPackage = JSON.parse(readFileSync("apps/desktop/package.json", "utf8"));
  const executableName = desktopPackage.build?.executableName;

  assert.equal(executableName, "codel00p-desktop");
  assert.match(executableName, /^[A-Za-z0-9._ -]+$/);
});

test("web docs list every desktop release asset", () => {
  const page = readFileSync("apps/landing/app/docs/desktop/page.tsx", "utf8");
  const nav = readFileSync("apps/landing/components/docs/nav.ts", "utf8");

  assert.match(nav, /Desktop app/);
  for (const target of desktopTargets) {
    assert.match(page, new RegExp(desktopAssetName(target)));
  }
});
