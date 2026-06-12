import { useEffect, useRef, useState, type ReactNode } from "react";
import {
  useClerk,
  useOrganization,
  useOrganizationList,
  useUser
} from "@clerk/clerk-react";

import { cn } from "@/lib/utils";

/** Shared dropdown shell: a trigger plus a dark, click-away panel. */
function Menu({
  trigger,
  children,
  align = "end"
}: {
  trigger: ReactNode;
  children: (close: () => void) => ReactNode;
  align?: "start" | "end";
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    function onPointerDown(event: MouseEvent) {
      if (ref.current && !ref.current.contains(event.target as Node)) {
        setOpen(false);
      }
    }
    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape") setOpen(false);
    }
    document.addEventListener("mousedown", onPointerDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onPointerDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div className="app-no-drag relative" ref={ref}>
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        className="flex items-center gap-2 rounded-full border border-border bg-card/50 px-2.5 py-1.5 text-sm text-foreground transition-colors hover:bg-card"
      >
        {trigger}
      </button>
      {open ? (
        <div
          className={cn(
            "absolute top-[calc(100%+8px)] z-50 min-w-56 overflow-hidden rounded-xl border border-border bg-popover p-1.5 shadow-2xl",
            align === "end" ? "right-0" : "left-0"
          )}
        >
          {children(() => setOpen(false))}
        </div>
      ) : null}
    </div>
  );
}

function Chevron() {
  return (
    <svg
      viewBox="0 0 24 24"
      className="size-3.5 text-muted-foreground"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <path d="m6 9 6 6 6-6" />
    </svg>
  );
}

function Avatar({ src, name }: { src?: string; name: string }) {
  if (src) {
    return <img src={src} alt="" className="size-6 rounded-full object-cover" />;
  }
  return (
    <span className="grid size-6 place-items-center rounded-full bg-brand/25 text-xs font-medium text-foreground">
      {name.charAt(0).toUpperCase()}
    </span>
  );
}

function MenuItem({
  onClick,
  children
}: {
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex w-full items-center gap-2 rounded-lg px-2.5 py-2 text-left text-sm text-muted-foreground transition-colors hover:bg-foreground/5 hover:text-foreground"
    >
      {children}
    </button>
  );
}

/** Custom organization switcher — dark, driven by Clerk's hooks. */
export function OrgMenu() {
  const { organization } = useOrganization();
  const { userMemberships, setActive, isLoaded } = useOrganizationList({
    userMemberships: { infinite: true }
  });
  const { openOrganizationProfile, openCreateOrganization } = useClerk();

  const memberships = userMemberships?.data ?? [];
  const activeName = organization?.name ?? "No organization";

  return (
    <Menu
      trigger={
        <>
          <span className="size-1.5 rounded-full bg-brand" />
          <span className="max-w-40 truncate">{activeName}</span>
          <Chevron />
        </>
      }
    >
      {(close) => (
        <>
          <p className="px-2.5 py-1.5 text-[0.65rem] uppercase tracking-wider text-muted-foreground/70">
            Organizations
          </p>
          {memberships.length === 0 ? (
            <p className="px-2.5 py-2 text-xs text-muted-foreground">
              {isLoaded ? "No organizations yet." : "Loading…"}
            </p>
          ) : (
            memberships.map((membership) => {
              const isActive = membership.organization.id === organization?.id;
              return (
                <button
                  key={membership.organization.id}
                  type="button"
                  onClick={() => {
                    if (setActive) {
                      void setActive({ organization: membership.organization.id });
                    }
                    close();
                  }}
                  className={cn(
                    "flex w-full items-center justify-between gap-3 rounded-lg px-2.5 py-2 text-sm transition-colors",
                    "text-muted-foreground hover:bg-foreground/5 hover:text-foreground",
                    isActive && "text-foreground"
                  )}
                >
                  <span className="truncate">{membership.organization.name}</span>
                  {isActive ? <span className="size-1.5 rounded-full bg-brand" /> : null}
                </button>
              );
            })
          )}
          <div className="my-1 h-px bg-border" />
          {organization ? (
            <MenuItem
              onClick={() => {
                close();
                openOrganizationProfile();
              }}
            >
              Manage organization
            </MenuItem>
          ) : null}
          <MenuItem
            onClick={() => {
              close();
              openCreateOrganization();
            }}
          >
            Create organization
          </MenuItem>
        </>
      )}
    </Menu>
  );
}

/** Custom user menu — dark, with account management and sign out. */
export function UserMenu() {
  const { user } = useUser();
  const { signOut, openUserProfile } = useClerk();

  const name =
    user?.firstName ??
    user?.primaryEmailAddress?.emailAddress ??
    user?.username ??
    "Account";
  const email = user?.primaryEmailAddress?.emailAddress;

  return (
    <Menu
      trigger={
        <>
          <Avatar src={user?.imageUrl} name={name} />
          <span className="max-w-32 truncate">{name}</span>
          <Chevron />
        </>
      }
    >
      {(close) => (
        <>
          <div className="flex items-center gap-2.5 border-b border-border px-2.5 pb-2.5 pt-1.5">
            <Avatar src={user?.imageUrl} name={name} />
            <div className="min-w-0">
              <p className="truncate text-sm font-medium text-foreground">{name}</p>
              {email ? (
                <p className="truncate text-xs text-muted-foreground">{email}</p>
              ) : null}
            </div>
          </div>
          <div className="mt-1 flex flex-col">
            <MenuItem
              onClick={() => {
                close();
                openUserProfile();
              }}
            >
              Manage account
            </MenuItem>
            <MenuItem
              onClick={() => {
                close();
                void signOut();
              }}
            >
              Sign out
            </MenuItem>
          </div>
        </>
      )}
    </Menu>
  );
}
