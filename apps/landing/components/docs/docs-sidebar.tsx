"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";

import { cn } from "@/lib/utils";
import { docsNav } from "@/components/docs/nav";

export function DocsSidebar() {
  const pathname = usePathname();

  return (
    <nav className="space-y-7">
      {docsNav.map((section, index) => (
        <div key={section.label ?? `section-${index}`}>
          {section.label ? (
            <p className="label mb-3 text-muted-foreground/60">
              {section.label}
            </p>
          ) : null}
          <ul className="border-l border-border/60">
            {section.items.map((item) => {
              const active = pathname === item.href;
              return (
                <li key={item.href}>
                  <Link
                    href={item.href}
                    className={cn(
                      "-ml-px block border-l py-1.5 pl-4 text-sm transition-colors",
                      active
                        ? "border-brand font-medium text-foreground"
                        : "border-transparent text-muted-foreground hover:border-border hover:text-foreground",
                    )}
                  >
                    {item.title}
                  </Link>
                </li>
              );
            })}
          </ul>
        </div>
      ))}
    </nav>
  );
}
