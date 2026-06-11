/**
 * Fixed, non-interactive backdrop for the whole page:
 *  - a soft luminous brand wash that slowly drifts,
 *  - a faint technical grid that fades toward the edges,
 *  - a fine grain layer to kill banding on the gradients,
 *  - a vignette to seat the content in the dark.
 */
export function GlowBackground() {
  return (
    <div
      aria-hidden
      className="pointer-events-none absolute inset-x-0 top-0 -z-10 h-[min(1040px,112vh)] overflow-hidden"
    >
      <div className="absolute inset-0 grid-veil opacity-[0.5]" />
      <div className="absolute inset-0 brand-glow animate-drift" />

      {/* horizon hairline under the hero */}
      <div className="absolute left-1/2 top-[58%] h-px w-[min(680px,80vw)] -translate-x-1/2 hairline opacity-60" />

      {/* grain */}
      <div
        className="absolute inset-0 opacity-[0.035] mix-blend-soft-light"
        style={{
          backgroundImage:
            "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='160' height='160'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.8' numOctaves='2' stitchTiles='stitch'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)'/%3E%3C/svg%3E\")",
        }}
      />

      {/* vignette */}
      <div
        className="absolute inset-0"
        style={{
          background:
            "radial-gradient(120% 90% at 50% 0%, transparent 55%, color-mix(in oklab, var(--background) 92%, black) 100%)",
        }}
      />
    </div>
  );
}
