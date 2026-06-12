import { cn } from "@/lib/utils";

/**
 * The codel00p glyph: a steady core with a node orbiting a thin ring — the loop
 * of work becoming memory becoming better work. Pure SVG/CSS so it stays crisp
 * at any size and respects reduced-motion.
 */
export function LoopMark({ className }: { className?: string }) {
  return (
    <span className={cn("relative inline-block", className)} aria-hidden>
      <svg viewBox="0 0 48 48" fill="none" className="size-full">
        <circle
          cx="24"
          cy="24"
          r="19"
          stroke="color-mix(in oklab, var(--brand) 55%, transparent)"
          strokeWidth="1"
        />
        <circle
          cx="24"
          cy="24"
          r="11"
          stroke="color-mix(in oklab, var(--foreground) 22%, transparent)"
          strokeWidth="1"
        />
        <circle cx="24" cy="24" r="3" fill="var(--brand)" />
        <g className="origin-center animate-spin-slow">
          <circle cx="24" cy="5" r="2.4" fill="var(--foreground)" />
        </g>
      </svg>
      <span className="absolute inset-0 -z-10 rounded-full bg-brand/30 blur-xl" />
    </span>
  );
}
