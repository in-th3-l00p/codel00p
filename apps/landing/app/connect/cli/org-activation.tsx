"use client";

import { useEffect, useMemo, useState } from "react";
import { useOrganizationList } from "@clerk/nextjs";

import { CliHandoff } from "./handoff";

/**
 * Activates the requested Clerk organization in the browser session, then
 * reloads the server handoff so `auth().getToken({ template: "cli" })` mints
 * the CLI token with that active org.
 */
export function CliOrgActivation({
  orgId,
  port,
  state
}: {
  orgId: string;
  port: string;
  state: string;
}) {
  const { isLoaded, setActive } = useOrganizationList();
  const [error, setError] = useState<string | null>(null);
  const continueUrl = useMemo(() => {
    const params = new URLSearchParams({
      port,
      state,
      org_id: orgId,
      activated: "1"
    });
    return `/connect/cli?${params.toString()}`;
  }, [orgId, port, state]);

  useEffect(() => {
    if (!isLoaded || !setActive) return;
    let cancelled = false;
    setActive({ organization: orgId })
      .then(() => {
        if (!cancelled) window.location.replace(continueUrl);
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : String(err));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [continueUrl, isLoaded, orgId, setActive]);

  if (error) {
    return <CliHandoff error={`Could not activate organization ${orgId}: ${error}`} />;
  }

  return <CliHandoff />;
}
