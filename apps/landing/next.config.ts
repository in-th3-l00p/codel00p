import type { NextConfig } from "next";

// The dashboard reads the codel00p-cloud service at runtime via CODEL00P_API_URL
// (set in the environment / on Vercel); @codel00p/sdk ships TS sources, so it is
// transpiled here rather than pre-built.
const nextConfig: NextConfig = {
  transpilePackages: ["@codel00p/sdk"]
};

export default nextConfig;
