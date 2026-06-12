import { execFile } from "node:child_process";
import { existsSync } from "node:fs";
import { resolve } from "node:path";
import { promisify } from "node:util";

const run = promisify(execFile);

export type LocalSession = {
  session_id: string;
  source: string;
  parent_session_id?: string | null;
  message_count: number;
  event_count: number;
};

export type LocalSessionsResult = {
  available: boolean;
  error?: string;
  sessions: LocalSession[];
};

/**
 * Resolves the `codel00p` CLI binary: an explicit `CODEL00P_BIN`, then the
 * workspace debug/release builds, then whatever is on PATH.
 */
function resolveBinary(): string {
  const fromEnv = process.env.CODEL00P_BIN;
  if (fromEnv && existsSync(fromEnv)) {
    return fromEnv;
  }
  const candidates = [
    resolve(process.cwd(), "../../core/target/debug/codel00p"),
    resolve(process.cwd(), "../../core/target/release/codel00p"),
    resolve(process.cwd(), "core/target/debug/codel00p"),
    resolve(process.cwd(), "core/target/release/codel00p")
  ];
  for (const candidate of candidates) {
    if (existsSync(candidate)) {
      return candidate;
    }
  }
  return "codel00p";
}

async function runJson<T>(args: string[]): Promise<T> {
  const binary = resolveBinary();
  const { stdout } = await run(binary, args, {
    timeout: 10_000,
    maxBuffer: 8 * 1024 * 1024
  });
  return JSON.parse(stdout || "[]") as T;
}

/**
 * Lists local agent sessions via the CLI. Failures (no binary, no local store)
 * degrade to an unavailable result rather than throwing, so the dashboard can
 * show a clear empty state.
 */
export async function localSessions(): Promise<LocalSessionsResult> {
  try {
    const sessions = await runJson<LocalSession[]>(["session", "list", "--json"]);
    return { available: true, sessions };
  } catch (error) {
    return {
      available: false,
      error: error instanceof Error ? error.message : String(error),
      sessions: []
    };
  }
}

export type EngineStatus = {
  /** Whether the codel00p CLI binary could be found and executed. */
  binaryFound: boolean;
};

/**
 * Detects whether the codel00p CLI is installed by running a command that needs
 * no local store (`config show`). ENOENT means the binary is missing; any other
 * failure means it exists but errored on this machine.
 */
export async function engineStatus(): Promise<EngineStatus> {
  const binary = resolveBinary();
  try {
    await run(binary, ["config", "show", "--json"], { timeout: 8_000 });
    return { binaryFound: true };
  } catch (error) {
    const code = (error as { code?: string } | undefined)?.code;
    return { binaryFound: code !== "ENOENT" };
  }
}
