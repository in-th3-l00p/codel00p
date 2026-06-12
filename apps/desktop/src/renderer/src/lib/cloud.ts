import { Codel00pClient, type TokenProvider } from "@codel00p/sdk";

export const cloudBaseUrl =
  import.meta.env.RENDERER_VITE_CODEL00P_API_URL ?? "http://localhost:8787";

/** Builds a cloud SDK client bound to a Clerk token provider. */
export function createCloudClient(getToken: TokenProvider): Codel00pClient {
  return new Codel00pClient({ baseUrl: cloudBaseUrl, getToken });
}
