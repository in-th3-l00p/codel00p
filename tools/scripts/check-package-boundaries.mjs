import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const root = process.cwd();
const workspaceRoots = ["apps", "packages"];
const dependencyFields = [
  "dependencies",
  "devDependencies",
  "peerDependencies",
  "optionalDependencies"
];

const packageFiles = [];
for (const workspaceRoot of workspaceRoots) {
  const workspacePath = join(root, workspaceRoot);
  if (!existsSync(workspacePath)) {
    continue;
  }

  for (const entry of readdirSync(workspacePath, { withFileTypes: true })) {
    if (entry.isDirectory()) {
      const packagePath = join(workspacePath, entry.name, "package.json");
      if (existsSync(packagePath)) {
        packageFiles.push(packagePath);
      }
    }
  }
}

const packages = packageFiles.map((packagePath) => {
  const manifest = JSON.parse(readFileSync(packagePath, "utf8"));
  return {
    path: packagePath,
    directory: packagePath.includes("/apps/") ? "apps" : "packages",
    manifest
  };
});

const names = new Set(packages.map((entry) => entry.manifest.name));
const errors = [];

for (const { path, directory, manifest } of packages) {
  for (const field of dependencyFields) {
    const dependencies = manifest[field] ?? {};
    for (const [name, version] of Object.entries(dependencies)) {
      if (version === "latest") {
        errors.push(`${manifest.name} uses latest for ${name} in ${field}`);
      }

      if (names.has(name) && version !== "workspace:*") {
        errors.push(`${manifest.name} must depend on ${name} via workspace:*`);
      }

      if (directory === "packages" && name.startsWith("@codel00p/")) {
        const target = packages.find((entry) => entry.manifest.name === name);
        if (target?.directory === "apps") {
          errors.push(`${manifest.name} package cannot depend on app ${name}`);
        }
      }
    }
  }

  if (manifest.name === "@codel00p/protocol-ts") {
    for (const field of dependencyFields) {
      const dependencies = manifest[field] ?? {};
      const dependencyNames = Object.keys(dependencies);
      if (dependencyNames.length > 0) {
        errors.push("@codel00p/protocol-ts must remain dependency-free");
      }
    }
  }

  if (manifest.name === "@codel00p/ui") {
    const dependencies = {
      ...manifest.dependencies,
      ...manifest.devDependencies,
      ...manifest.optionalDependencies
    };
    if (dependencies["@codel00p/sdk"]) {
      errors.push("@codel00p/ui must not depend on @codel00p/sdk");
    }
  }

  if (!manifest.private) {
    errors.push(`${manifest.name} must stay private until release policy exists (${path})`);
  }
}

if (errors.length > 0) {
  console.error("Package boundary check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log(`Package boundary check passed for ${packages.length} packages.`);
