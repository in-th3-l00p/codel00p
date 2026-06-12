import { cn } from "@/lib/utils";

/**
 * Full-surface ambient backdrop, ported from the landing site. Layers, back to
 * front: a faint technical grid that fades toward the edges, the luminous brand
 * glow drifting slowly, a fine grain layer to kill banding, and a vignette that
 * seats the content in the dark. There is no scroll in the desktop shell, so the
 * landing's scroll-linked parallax is dropped.
 */
export function GlowBackground({ className }: { className?: string }) {
  return (
    <div
      aria-hidden
      className={cn(
        "pointer-events-none absolute inset-0 -z-10 overflow-hidden",
        className
      )}
    >
      <div className="absolute inset-0 grid-veil opacity-50" />
      <div className="absolute inset-0 brand-glow animate-drift" />
      <div className="absolute inset-0 grain opacity-[0.04] mix-blend-soft-light" />
      <div className="absolute inset-0 vignette" />
    </div>
  );
}
