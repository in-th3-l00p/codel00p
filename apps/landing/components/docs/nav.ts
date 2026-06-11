export type DocLink = { title: string; href: string };
export type DocSection = { label: string | null; items: DocLink[] };

/**
 * The documentation table of contents. Order here drives the sidebar, the
 * previous/next pager, and the breadcrumb group shown on each page.
 */
export const docsNav: DocSection[] = [
  {
    label: null,
    items: [{ title: "Introduction", href: "/docs" }],
  },
  {
    label: "Getting started",
    items: [
      { title: "Installation", href: "/docs/installation" },
      { title: "Quick start", href: "/docs/quick-start" },
    ],
  },
  {
    label: "Architecture",
    items: [
      { title: "Overview", href: "/docs/architecture" },
      { title: "The harness", href: "/docs/architecture/harness" },
      { title: "Project memory", href: "/docs/architecture/memory" },
      { title: "Providers & routing", href: "/docs/architecture/providers" },
      { title: "Storage & sessions", href: "/docs/architecture/storage" },
    ],
  },
  {
    label: "Guides",
    items: [
      { title: "Curating memory", href: "/docs/guides/memory" },
      { title: "Connecting MCP tools", href: "/docs/guides/mcp" },
    ],
  },
  {
    label: "Reference",
    items: [{ title: "CLI commands", href: "/docs/reference/cli" }],
  },
];

/** Flattened, ordered list of every page — used for previous/next navigation. */
export const docsPages: DocLink[] = docsNav.flatMap((section) => section.items);

/** The section label a given page belongs to, for the page's eyebrow. */
export function groupFor(href: string): string | null {
  const section = docsNav.find((s) => s.items.some((i) => i.href === href));
  return section?.label ?? null;
}
