/**
 * Full-viewport, fixed backdrop shared by every page. Because it is fixed, the
 * content scrolls over a stationary glow (a parallax effect), and the same
 * ambient treatment is visible from the top of the hero to the footer with no
 * seam. Layers, back to front:
 *  - a faint technical grid that fades toward the edges,
 *  - the luminous brand glow, drifting slowly and parallaxing on scroll,
 *  - a fine grain layer to kill banding on the gradients,
 *  - a vignette that seats the content in the dark.
 */
export function GlowBackground() {
  return (
    <div
      aria-hidden
      className="pointer-events-none fixed inset-0 -z-10 overflow-hidden"
    >
      <div className="absolute inset-0 grid-veil opacity-50" />

      <div className="parallax absolute inset-0 will-change-transform">
        <div className="absolute inset-0 brand-glow animate-drift" />
      </div>

      <div className="absolute inset-0 grain opacity-[0.04] mix-blend-soft-light" />
      <div className="absolute inset-0 vignette" />
    </div>
  );
}
