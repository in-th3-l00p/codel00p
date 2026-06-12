"use client";

import { useRef, useState, useTransition } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { createProjectAction } from "@/app/dashboard/actions";

/** Admin-only inline form to add a project to the org's shared pool. */
export function NewProjectForm() {
  const [pending, startTransition] = useTransition();
  const [error, setError] = useState<string | null>(null);
  const formRef = useRef<HTMLFormElement>(null);

  return (
    <form
      ref={formRef}
      action={(formData) =>
        startTransition(async () => {
          setError(null);
          const result = await createProjectAction(formData);
          if (result?.error) {
            setError(result.error);
          } else {
            formRef.current?.reset();
          }
        })
      }
      className="flex flex-col gap-2"
    >
      <div className="flex flex-col gap-2 sm:flex-row">
        <Input name="name" placeholder="New project name" required className="sm:flex-1" />
        <Input
          name="repository_url"
          placeholder="Repository URL (optional)"
          className="sm:flex-1"
        />
        <Button type="submit" size="lg" disabled={pending} className="rounded-full">
          {pending ? "Creating…" : "Create project"}
        </Button>
      </div>
      {error ? (
        <p className="text-xs text-destructive" role="alert">
          {error}
        </p>
      ) : null}
    </form>
  );
}
