import { useCallback, useEffect, useMemo, useState } from "react";
import { useAuth, useOrganizationList } from "@clerk/clerk-react";
import { Codel00pApiError } from "@codel00p/sdk";

import { cloudBaseUrl, createCloudClient } from "./cloud";
import type { AgentItem, OrgItem, ProjectItem } from "./dashboard-types";

type LoadState = "idle" | "loading" | "ready" | "error";

export type DashboardData = {
  orgs: OrgItem[];
  projects: ProjectItem[];
  agents: AgentItem[];
  cloud: { state: LoadState; error?: string };
  local: {
    available: boolean;
    binaryFound: boolean;
    state: LoadState;
    error?: string;
  };
  /** Whether the live SSE stream is currently connected. */
  live: boolean;
  refresh: () => void;
};

export function useDashboardData(): DashboardData {
  const { getToken, orgId, isSignedIn } = useAuth();
  const { userMemberships } = useOrganizationList({
    userMemberships: { infinite: true }
  });

  const [cloudProjects, setCloudProjects] = useState<ProjectItem[]>([]);
  const [cloudAgents, setCloudAgents] = useState<AgentItem[]>([]);
  const [cloudState, setCloudState] = useState<LoadState>("idle");
  const [cloudError, setCloudError] = useState<string | undefined>(undefined);

  const [localAgents, setLocalAgents] = useState<AgentItem[]>([]);
  const [localState, setLocalState] = useState<LoadState>("idle");
  const [localAvailable, setLocalAvailable] = useState(false);
  const [localBinaryFound, setLocalBinaryFound] = useState(true);
  const [localError, setLocalError] = useState<string | undefined>(undefined);

  const [live, setLive] = useState(false);

  const [nonce, setNonce] = useState(0);
  const refresh = useCallback(() => setNonce((value) => value + 1), []);

  // Detect whether the local CLI engine is installed (separate from whether it
  // has any data) so the UI can offer an install path.
  useEffect(() => {
    let cancelled = false;
    const bridge = window.codel00p?.local;
    if (!bridge) {
      setLocalBinaryFound(false);
      return;
    }
    bridge
      .engineStatus()
      .then((status) => {
        if (!cancelled) setLocalBinaryFound(status.binaryFound);
      })
      .catch(() => {
        if (!cancelled) setLocalBinaryFound(false);
      });
    return () => {
      cancelled = true;
    };
  }, [nonce]);

  // Live updates: subscribe to the cloud SSE stream and refetch on changes.
  // Replaces manual refresh. Reconnects with a short backoff.
  useEffect(() => {
    if (!isSignedIn) {
      return;
    }
    let cancelled = false;
    const controller = new AbortController();

    async function streamOnce(): Promise<void> {
      const token = await getToken();
      const response = await fetch(`${cloudBaseUrl}/events`, {
        headers: {
          authorization: token ? `Bearer ${token}` : "",
          accept: "text/event-stream"
        },
        signal: controller.signal
      });
      if (!response.ok || !response.body) {
        throw new Error(`events ${response.status}`);
      }
      setLive(true);
      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = "";
      while (!cancelled) {
        const { value, done } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        let boundary = buffer.indexOf("\n\n");
        while (boundary >= 0) {
          const frame = buffer.slice(0, boundary);
          buffer = buffer.slice(boundary + 2);
          if (frame.includes('"entity"')) {
            refresh();
          }
          boundary = buffer.indexOf("\n\n");
        }
      }
    }

    async function loop(): Promise<void> {
      while (!cancelled) {
        try {
          await streamOnce();
        } catch {
          // network error / aborted — fall through to reconnect
        }
        setLive(false);
        if (cancelled) break;
        await new Promise((resolve) => setTimeout(resolve, 3000));
      }
    }

    void loop();
    return () => {
      cancelled = true;
      controller.abort();
      setLive(false);
    };
  }, [getToken, orgId, isSignedIn, refresh]);

  // Cloud projects for the active organization.
  useEffect(() => {
    let cancelled = false;
    async function load() {
      if (!isSignedIn) {
        return;
      }
      setCloudState("loading");
      try {
        const client = createCloudClient(() => getToken());
        const projects = await client.listProjects();
        if (cancelled) return;
        setCloudProjects(
          projects.map((project) => ({
            id: project.id,
            name: project.name,
            source: "cloud" as const,
            org: project.org_id,
            slug: project.slug,
            repositoryUrl: project.repository_url
          }))
        );

        // Agents are project-scoped; gather the active org's shared pool.
        const agentLists = await Promise.all(
          projects.map((project) =>
            client.listAgents(project.id).catch(() => [])
          )
        );
        if (cancelled) return;
        setCloudAgents(
          agentLists.flat().map((agent) => ({
            id: agent.id,
            source: "cloud" as const,
            label: agent.name,
            origin: `${agent.provider}/${agent.model}`
          }))
        );

        setCloudState("ready");
        setCloudError(undefined);
      } catch (error) {
        if (cancelled) return;
        setCloudProjects([]);
        setCloudAgents([]);
        setCloudState("error");
        setCloudError(
          error instanceof Codel00pApiError ? error.message : String(error)
        );
      }
    }
    load();
    return () => {
      cancelled = true;
    };
  }, [getToken, orgId, isSignedIn, nonce]);

  // Local agent sessions via the IPC bridge.
  useEffect(() => {
    let cancelled = false;
    async function load() {
      const bridge = window.codel00p?.local;
      if (!bridge) {
        setLocalAvailable(false);
        setLocalState("ready");
        return;
      }
      setLocalState("loading");
      const result = await bridge.sessions();
      if (cancelled) return;
      setLocalAvailable(result.available);
      setLocalError(result.error);
      setLocalAgents(
        result.sessions.map((session) => ({
          id: session.session_id,
          source: "local" as const,
          label: session.session_id,
          origin: session.source,
          messages: session.message_count,
          events: session.event_count
        }))
      );
      setLocalState("ready");
    }
    load();
    return () => {
      cancelled = true;
    };
  }, [nonce]);

  const orgs = useMemo<OrgItem[]>(() => {
    const cloudOrgs: OrgItem[] = (userMemberships?.data ?? []).map((membership) => ({
      id: membership.organization.id,
      name: membership.organization.name,
      source: "cloud",
      role: roleLabel(membership.role)
    }));
    const localOrgs: OrgItem[] = localAvailable
      ? [{ id: "local", name: "Local workspace", source: "local" }]
      : [];
    return [...localOrgs, ...cloudOrgs];
  }, [userMemberships?.data, localAvailable]);

  const projects = useMemo<ProjectItem[]>(() => {
    const localProjects: ProjectItem[] = localAvailable
      ? [{ id: "local-workspace", name: "Local workspace", source: "local", org: "local" }]
      : [];
    return [...localProjects, ...cloudProjects];
  }, [cloudProjects, localAvailable]);

  const agents = useMemo<AgentItem[]>(
    () => [...localAgents, ...cloudAgents],
    [localAgents, cloudAgents]
  );

  return {
    orgs,
    projects,
    agents,
    cloud: { state: cloudState, error: cloudError },
    local: {
      available: localAvailable,
      binaryFound: localBinaryFound,
      state: localState,
      error: localError
    },
    live,
    refresh
  };
}

function roleLabel(role: string): string {
  const normalized = role.replace(/^org:/, "");
  return normalized === "basic_member" ? "member" : normalized;
}
