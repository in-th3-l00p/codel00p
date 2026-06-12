"use client";

import { useEffect, useState, type FormEvent } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { useAuth, useSignIn } from "@clerk/nextjs";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { GlowBackground } from "@/components/site/glow-background";
import { LoopMark } from "@/components/site/loop-mark";
import { cn } from "@/lib/utils";
import {
  ArrowRightIcon,
  GitHubIcon,
  GoogleIcon,
  MailIcon,
  SpinnerIcon
} from "./icons";

type Step = "identifier" | "code";
type OAuthStrategy = "oauth_google" | "oauth_github";

/** The page the sign-in flow lands on once a session is established. */
const AFTER_SIGN_IN = "/dashboard";

/** Pulls a human-readable message out of a Clerk API error, with a fallback. */
function clerkErrorMessage(error: unknown, fallback: string): string {
  if (error && typeof error === "object" && "errors" in error) {
    const first = (
      error as { errors?: Array<{ longMessage?: string; message?: string }> }
    ).errors?.[0];
    if (first?.longMessage) return first.longMessage;
    if (first?.message) return first.message;
  }
  return fallback;
}

export function SignInScreen() {
  const router = useRouter();
  const { isLoaded, signIn, setActive } = useSignIn();
  const { isSignedIn } = useAuth();

  const [step, setStep] = useState<Step>("identifier");
  const [email, setEmail] = useState("");
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [oauthBusy, setOauthBusy] = useState<OAuthStrategy | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Already authenticated visitors skip straight to the dashboard.
  useEffect(() => {
    if (isSignedIn) router.replace(AFTER_SIGN_IN);
  }, [isSignedIn, router]);

  async function continueWithOAuth(strategy: OAuthStrategy) {
    if (!isLoaded || oauthBusy) return;
    setError(null);
    setOauthBusy(strategy);
    try {
      await signIn.authenticateWithRedirect({
        strategy,
        redirectUrl: "/sign-in/sso-callback",
        redirectUrlComplete: AFTER_SIGN_IN
      });
    } catch (err) {
      setOauthBusy(null);
      setError(clerkErrorMessage(err, "Couldn't start that sign-in. Try again."));
    }
  }

  async function submitEmail(event: FormEvent) {
    event.preventDefault();
    if (!isLoaded || busy) return;
    setError(null);
    setBusy(true);
    try {
      const attempt = await signIn.create({ identifier: email.trim() });
      const factor = attempt.supportedFirstFactors?.find(
        (candidate) =>
          candidate.strategy === "email_code" && "emailAddressId" in candidate
      );
      if (!factor || !("emailAddressId" in factor)) {
        throw new Error("Email-code sign-in isn't available for this address.");
      }
      await signIn.prepareFirstFactor({
        strategy: "email_code",
        emailAddressId: factor.emailAddressId
      });
      setStep("code");
    } catch (err) {
      setError(clerkErrorMessage(err, "We couldn't send a code. Try again."));
    } finally {
      setBusy(false);
    }
  }

  async function submitCode(event: FormEvent) {
    event.preventDefault();
    if (!isLoaded || busy) return;
    setError(null);
    setBusy(true);
    try {
      const attempt = await signIn.attemptFirstFactor({
        strategy: "email_code",
        code: code.trim()
      });
      if (attempt.status === "complete") {
        await setActive({ session: attempt.createdSessionId });
        router.push(AFTER_SIGN_IN);
      } else {
        setError("That code didn't complete sign in. Try again.");
      }
    } catch (err) {
      setError(clerkErrorMessage(err, "That code wasn't right. Try again."));
    } finally {
      setBusy(false);
    }
  }

  function resetToEmail() {
    setStep("identifier");
    setCode("");
    setError(null);
  }

  return (
    <main className="relative grid min-h-screen grid-cols-1 overflow-hidden lg:grid-cols-[1.05fr_1fr]">
      <GlowBackground />

      <BrandPanel />

      {/* Form column */}
      <section className="relative flex items-center justify-center px-6 py-12 sm:px-10">
        <div className="rise w-full max-w-sm">
          <Link
            href="/"
            className="mb-8 inline-flex items-center gap-2.5 lg:hidden"
          >
            <LoopMark className="size-9" />
            <span className="font-hand text-2xl leading-none">codel00p</span>
          </Link>

          <p className="label text-brand/80">
            {step === "identifier" ? "Sign in" : "Check your email"}
          </p>
          <h2 className="mt-3 text-3xl font-medium tracking-tight text-foreground">
            {step === "identifier" ? (
              "Welcome back"
            ) : (
              <>
                Enter your{" "}
                <span className="font-hand text-[1.35em] leading-none text-brand">
                  code
                </span>
              </>
            )}
          </h2>
          <p className="mt-3 text-sm leading-relaxed text-muted-foreground">
            {step === "identifier"
              ? "Continue with a provider, or use a one-time email code."
              : `We sent a 6-digit code to ${email}.`}
          </p>

          {step === "identifier" ? (
            <>
              <div className="mt-8 grid grid-cols-2 gap-3">
                <Button
                  type="button"
                  variant="outline"
                  size="lg"
                  disabled={oauthBusy !== null}
                  onClick={() => continueWithOAuth("oauth_google")}
                  className="justify-center gap-2.5"
                >
                  {oauthBusy === "oauth_google" ? (
                    <SpinnerIcon className="size-4" />
                  ) : (
                    <>
                      <GoogleIcon className="size-[18px]" />
                      Google
                    </>
                  )}
                </Button>
                <Button
                  type="button"
                  variant="outline"
                  size="lg"
                  disabled={oauthBusy !== null}
                  onClick={() => continueWithOAuth("oauth_github")}
                  className="justify-center gap-2.5"
                >
                  {oauthBusy === "oauth_github" ? (
                    <SpinnerIcon className="size-4" />
                  ) : (
                    <>
                      <GitHubIcon className="size-[18px]" />
                      GitHub
                    </>
                  )}
                </Button>
              </div>

              <div className="mt-5 flex items-center gap-3 text-muted-foreground/60">
                <span className="h-px flex-1 bg-border" />
                <span className="label text-[0.6rem]">or email code</span>
                <span className="h-px flex-1 bg-border" />
              </div>

              <form onSubmit={submitEmail} className="mt-5 flex flex-col gap-3">
                <label className="flex flex-col gap-2">
                  <span className="label text-muted-foreground/80">Email</span>
                  <div className="relative">
                    <MailIcon className="pointer-events-none absolute left-3.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground/70" />
                    <Input
                      type="email"
                      autoComplete="email"
                      required
                      placeholder="you@team.dev"
                      value={email}
                      onChange={(event) => setEmail(event.target.value)}
                      className="pl-10"
                    />
                  </div>
                </label>

                <Button
                  type="submit"
                  size="lg"
                  disabled={busy || !email.trim()}
                  className="mt-1 w-full rounded-full"
                >
                  {busy ? (
                    <SpinnerIcon className="size-4" />
                  ) : (
                    <>
                      Continue with email
                      <ArrowRightIcon className="size-4" />
                    </>
                  )}
                </Button>
              </form>
            </>
          ) : (
            <form onSubmit={submitCode} className="mt-8 flex flex-col gap-3">
              <label className="flex flex-col gap-2">
                <span className="label text-muted-foreground/80">
                  Verification code
                </span>
                <Input
                  inputMode="numeric"
                  autoComplete="one-time-code"
                  required
                  placeholder="000000"
                  value={code}
                  onChange={(event) => setCode(event.target.value)}
                  className="text-center font-mono text-lg tracking-[0.5em]"
                  maxLength={6}
                  autoFocus
                />
              </label>

              <Button
                type="submit"
                size="lg"
                disabled={busy || code.trim().length < 6}
                className="mt-1 w-full rounded-full"
              >
                {busy ? <SpinnerIcon className="size-4" /> : "Sign in"}
              </Button>

              <button
                type="button"
                onClick={resetToEmail}
                className="mt-1 text-center text-xs text-muted-foreground transition-colors hover:text-foreground"
              >
                Use a different email
              </button>
            </form>
          )}

          {error ? (
            <p
              role="alert"
              className="mt-5 rounded-lg border border-destructive/30 bg-destructive/10 px-3.5 py-2.5 text-sm text-destructive"
            >
              {error}
            </p>
          ) : null}

          <p className="mt-8 text-xs leading-relaxed text-muted-foreground/70">
            By continuing you agree to keep your team&apos;s project memory
            reviewed and secure.
          </p>
        </div>
      </section>
    </main>
  );
}

/** The branded left column — the landing hero distilled into a sign-in panel. */
function BrandPanel() {
  const features = [
    "Reviewed, durable project memory",
    "Workspace-safe agent execution",
    "Any provider, one contract"
  ];

  return (
    <aside className="relative hidden flex-col justify-between overflow-hidden border-r border-border px-12 py-14 lg:flex">
      <div className="absolute inset-0 -z-10 brand-glow opacity-90 animate-drift" />

      <Link href="/" className="flex items-center gap-3">
        <LoopMark className="size-9" />
        <span className="label text-muted-foreground">codel00p · cloud</span>
      </Link>

      <div className="rise max-w-md">
        <h1
          className="font-hand leading-[0.95] text-foreground"
          style={{
            fontSize: "clamp(3.5rem, 8vw, 6rem)",
            textShadow:
              "0 0 80px color-mix(in oklab, var(--brand) 45%, transparent)"
          }}
        >
          codel00p
        </h1>
        <p className="mt-5 text-balance text-2xl font-medium tracking-tight text-foreground">
          The coding agent that remembers.
        </p>
        <p className="mt-4 max-w-sm text-balance text-sm leading-relaxed text-muted-foreground">
          Manage teams, projects, provider policy, and shared knowledge from one
          control surface — the same memory your agents use everywhere.
        </p>
      </div>

      <ul className="flex flex-col gap-3">
        {features.map((feature) => (
          <li
            key={feature}
            className="flex items-center gap-3 text-sm text-muted-foreground"
          >
            <span className={cn("size-1.5 rounded-full bg-brand")} />
            {feature}
          </li>
        ))}
      </ul>
    </aside>
  );
}
