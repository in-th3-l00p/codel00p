import { clerkMiddleware, createRouteMatcher } from "@clerk/nextjs/server";

/**
 * The dashboard and project surfaces require a session; the landing page,
 * sign-in flow, and desktop handoff stay public (the handoff gates itself).
 */
const isProtectedRoute = createRouteMatcher(["/dashboard(.*)", "/projects(.*)"]);

export default clerkMiddleware(async (auth, request) => {
  if (isProtectedRoute(request)) {
    // Send unauthenticated visitors to the in-app sign-in, not the hosted portal.
    await auth.protect({
      unauthenticatedUrl: new URL("/sign-in", request.url).toString()
    });
  }
});

export const config = {
  matcher: [
    // Run on everything except Next internals and static files, plus API routes.
    "/((?!_next|[^?]*\\.(?:html?|css|js(?!on)|jpe?g|webp|png|gif|svg|ttf|woff2?|ico|csv|docx?|xlsx?|zip|webmanifest)).*)",
    "/(api|trpc)(.*)"
  ]
};
