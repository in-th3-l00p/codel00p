import type { ProjectRef } from "@codel00p/protocol-ts";

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
}
