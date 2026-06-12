"use server";

import { revalidatePath } from "next/cache";
import { auth } from "@clerk/nextjs/server";

import { cloudClient } from "@/lib/api";

/**
 * Creates a project in the active org's shared pool. The cloud service enforces
 * org-admin; this also gates here so non-admins never reach the call.
 */
export async function createProjectAction(formData: FormData) {
  const { orgRole } = await auth();
  if (orgRole !== "org:admin" && orgRole !== "admin") {
    return { error: "Only organization admins can create projects." };
  }
  const name = String(formData.get("name") ?? "").trim();
  if (!name) {
    return { error: "Enter a project name." };
  }
  const repositoryUrl = String(formData.get("repository_url") ?? "").trim();

  const client = await cloudClient();
  await client.createProject({
    name,
    repository_url: repositoryUrl || undefined
  });
  revalidatePath("/dashboard");
  return { ok: true };
}
