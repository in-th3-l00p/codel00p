import {
  providerPolicyPresetById,
  providerPolicyPresets,
  type Agent,
  type AgentUpdate,
  type McpServer,
  type McpServerUpdate,
  type McpTransport,
  type MemoryAuditEntry,
  type MemoryEntry,
  type MemoryStatus,
  type NewAgent,
  type NewMcpServer,
  type NewMemoryCandidate,
  type NewProject,
  type Project,
  type ProjectRef,
  type ProjectUpdate,
  type ProviderPolicyPreset,
  type ProviderPolicyPresetId,
  type Viewer
} from "@codel00p/protocol-ts";

export {
  providerPolicyPresetById,
  providerPolicyPresets,
  type Agent,
  type AgentUpdate,
  type McpServer,
  type McpServerUpdate,
  type McpTransport,
  type MemoryAuditEntry,
  type MemoryEntry,
  type MemoryStatus,
  type NewAgent,
  type NewMcpServer,
  type NewMemoryCandidate,
  type NewProject,
  type Project,
  type ProjectRef,
  type ProjectUpdate,
  type ProviderPolicyPreset,
  type ProviderPolicyPresetId,
  type Viewer
} from "@codel00p/protocol-ts";

/**
 * Resolves the current Clerk session token. Both the desktop renderer and the
 * Next.js cloud app supply one — the SDK stays decoupled from any auth library.
 */
export type TokenProvider = () =>
  | string
  | null
  | undefined
  | Promise<string | null | undefined>;

export type Codel00pClientOptions = {
  baseUrl: string;
  /** Returns the bearer token to attach to authenticated requests. */
  getToken?: TokenProvider;
  /** Injectable fetch, for testing or non-browser runtimes. */
  fetch?: typeof fetch;
};

export class Codel00pApiError extends Error {
  readonly status: number;
  readonly code: string;

  constructor(status: number, code: string, message: string) {
    super(message);
    this.name = "Codel00pApiError";
    this.status = status;
    this.code = code;
  }
}

export class Codel00pClient {
  readonly baseUrl: string;
  readonly #getToken?: TokenProvider;
  readonly #fetch: typeof fetch;

  constructor(options: Codel00pClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/$/, "");
    this.#getToken = options.getToken;
    // Bind to the global so calling it as a method (`this.#fetch(...)`) doesn't
    // detach native `fetch` from `window` ("Illegal invocation").
    this.#fetch = options.fetch ?? globalThis.fetch.bind(globalThis);
  }

  projectUrl(project: ProjectRef): string {
    return `${this.baseUrl}/projects/${project.project_id}`;
  }

  providerPolicyPresets(): readonly ProviderPolicyPreset[] {
    return providerPolicyPresets;
  }

  providerPolicyPreset(id: ProviderPolicyPresetId): ProviderPolicyPreset {
    const preset = providerPolicyPresetById(id);
    if (!preset) {
      throw new Error(`unknown provider policy preset: ${id}`);
    }

    return preset;
  }

  /** `GET /me` — the authenticated caller and their active organization. */
  me(): Promise<Viewer> {
    return this.request<Viewer>("/me");
  }

  /** `GET /projects` — projects owned by the caller's active organization. */
  listProjects(): Promise<Project[]> {
    return this.request<Project[]>("/projects");
  }

  /** `POST /projects` — create a project (requires an org admin). */
  createProject(body: NewProject): Promise<Project> {
    return this.request<Project>("/projects", { method: "POST", body });
  }

  /** Push a memory candidate into a project's review queue. */
  pushMemoryCandidate(
    projectId: string,
    body: NewMemoryCandidate
  ): Promise<MemoryEntry> {
    return this.request<MemoryEntry>(`${this.memoryPath(projectId)}`, {
      method: "POST",
      body
    });
  }

  /** List a project's memory, optionally filtered to a single status. */
  listMemory(projectId: string, status?: MemoryStatus): Promise<MemoryEntry[]> {
    const query = status ? `?status=${encodeURIComponent(status)}` : "";
    return this.request<MemoryEntry[]>(`${this.memoryPath(projectId)}${query}`);
  }

  /** Approve a memory candidate (requires an org admin). */
  approveMemory(projectId: string, memoryId: string): Promise<MemoryEntry> {
    return this.request<MemoryEntry>(
      `${this.memoryPath(projectId)}/${encodeURIComponent(memoryId)}/approve`,
      { method: "POST" }
    );
  }

  /** Reject a memory candidate (requires an org admin). */
  rejectMemory(projectId: string, memoryId: string): Promise<MemoryEntry> {
    return this.request<MemoryEntry>(
      `${this.memoryPath(projectId)}/${encodeURIComponent(memoryId)}/reject`,
      { method: "POST" }
    );
  }

  /** Fetch the review audit trail for a memory entry. */
  memoryAudit(
    projectId: string,
    memoryId: string
  ): Promise<MemoryAuditEntry[]> {
    return this.request<MemoryAuditEntry[]>(
      `${this.memoryPath(projectId)}/${encodeURIComponent(memoryId)}/audit`
    );
  }

  /** RAG retrieval — approved memory ranked by relevance to a query. */
  searchMemory(
    projectId: string,
    query: string,
    limit?: number
  ): Promise<MemoryEntry[]> {
    const params = new URLSearchParams({ q: query });
    if (limit !== undefined) {
      params.set("limit", String(limit));
    }
    return this.request<MemoryEntry[]>(
      `${this.memoryPath(projectId)}/search?${params.toString()}`
    );
  }

  // --- Projects ---

  getProject(projectId: string): Promise<Project> {
    return this.request<Project>(this.projectPath(projectId));
  }

  updateProject(projectId: string, body: ProjectUpdate): Promise<Project> {
    return this.request<Project>(this.projectPath(projectId), {
      method: "PATCH",
      body
    });
  }

  deleteProject(projectId: string): Promise<void> {
    return this.request<void>(this.projectPath(projectId), { method: "DELETE" });
  }

  // --- Agents ---

  listAgents(projectId: string): Promise<Agent[]> {
    return this.request<Agent[]>(this.agentsPath(projectId));
  }

  createAgent(projectId: string, body: NewAgent): Promise<Agent> {
    return this.request<Agent>(this.agentsPath(projectId), {
      method: "POST",
      body
    });
  }

  getAgent(projectId: string, agentId: string): Promise<Agent> {
    return this.request<Agent>(
      `${this.agentsPath(projectId)}/${encodeURIComponent(agentId)}`
    );
  }

  updateAgent(
    projectId: string,
    agentId: string,
    body: AgentUpdate
  ): Promise<Agent> {
    return this.request<Agent>(
      `${this.agentsPath(projectId)}/${encodeURIComponent(agentId)}`,
      { method: "PATCH", body }
    );
  }

  deleteAgent(projectId: string, agentId: string): Promise<void> {
    return this.request<void>(
      `${this.agentsPath(projectId)}/${encodeURIComponent(agentId)}`,
      { method: "DELETE" }
    );
  }

  // --- MCP servers ---

  listMcpServers(projectId: string): Promise<McpServer[]> {
    return this.request<McpServer[]>(this.mcpPath(projectId));
  }

  createMcpServer(projectId: string, body: NewMcpServer): Promise<McpServer> {
    return this.request<McpServer>(this.mcpPath(projectId), {
      method: "POST",
      body
    });
  }

  getMcpServer(projectId: string, serverId: string): Promise<McpServer> {
    return this.request<McpServer>(
      `${this.mcpPath(projectId)}/${encodeURIComponent(serverId)}`
    );
  }

  updateMcpServer(
    projectId: string,
    serverId: string,
    body: McpServerUpdate
  ): Promise<McpServer> {
    return this.request<McpServer>(
      `${this.mcpPath(projectId)}/${encodeURIComponent(serverId)}`,
      { method: "PATCH", body }
    );
  }

  deleteMcpServer(projectId: string, serverId: string): Promise<void> {
    return this.request<void>(
      `${this.mcpPath(projectId)}/${encodeURIComponent(serverId)}`,
      { method: "DELETE" }
    );
  }

  private projectPath(projectId: string): string {
    return `/projects/${encodeURIComponent(projectId)}`;
  }

  private agentsPath(projectId: string): string {
    return `${this.projectPath(projectId)}/agents`;
  }

  private mcpPath(projectId: string): string {
    return `${this.projectPath(projectId)}/mcp-servers`;
  }

  private memoryPath(projectId: string): string {
    return `/projects/${encodeURIComponent(projectId)}/memory`;
  }

  async #authHeaders(): Promise<Record<string, string>> {
    if (!this.#getToken) {
      return {};
    }
    const token = await this.#getToken();
    return token ? { authorization: `Bearer ${token}` } : {};
  }

  private async request<T>(
    path: string,
    init: { method?: string; body?: unknown } = {}
  ): Promise<T> {
    const headers: Record<string, string> = {
      accept: "application/json",
      ...(await this.#authHeaders())
    };
    if (init.body !== undefined) {
      headers["content-type"] = "application/json";
    }

    const response = await this.#fetch(`${this.baseUrl}${path}`, {
      method: init.method ?? "GET",
      headers,
      body: init.body !== undefined ? JSON.stringify(init.body) : undefined
    });

    if (!response.ok) {
      let code = "error";
      let message = response.statusText;
      try {
        const payload = (await response.json()) as {
          error?: string;
          message?: string;
        };
        code = payload.error ?? code;
        message = payload.message ?? message;
      } catch {
        // Non-JSON error body; keep the status text.
      }
      throw new Codel00pApiError(response.status, code, message);
    }

    if (response.status === 204) {
      return undefined as T;
    }
    return (await response.json()) as T;
  }
}
