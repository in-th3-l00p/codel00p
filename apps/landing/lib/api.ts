import "server-only";

import { auth } from "@clerk/nextjs/server";
import { Codel00pClient } from "@codel00p/sdk";

const baseUrl = process.env.CODEL00P_API_URL ?? "http://localhost:8787";

/**
 * Builds an SDK client bound to the current request's Clerk session token, so
 * server components and actions talk to the Rust cloud service as the caller.
 */
export async function cloudClient(): Promise<Codel00pClient> {
  const { getToken } = await auth();
  const token = await getToken();
  return new Codel00pClient({ baseUrl, getToken: async () => token });
}
