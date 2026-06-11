import {
  providerPolicyPresetById,
  providerPolicyPresets,
  type ProjectRef,
  type ProviderPolicyPreset,
  type ProviderPolicyPresetId
} from "@codel00p/protocol-ts";

export {
  providerPolicyPresetById,
  providerPolicyPresets,
  type ProviderPolicyPreset,
  type ProviderPolicyPresetId
} from "@codel00p/protocol-ts";

export type Codel00pClientOptions = {
  baseUrl: string;
};

export class Codel00pClient {
  readonly baseUrl: string;

  constructor(options: Codel00pClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/$/, "");
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
}
