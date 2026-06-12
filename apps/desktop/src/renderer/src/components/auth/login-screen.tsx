import { useState, type FormEvent } from "react";
import { useSignIn } from "@clerk/clerk-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { GlowBackground } from "@/components/site/glow-background";
import { LoopMark } from "@/components/site/loop-mark";
import { cn } from "@/lib/utils";
import { ArrowRightIcon, BrowserIcon, MailIcon, SpinnerIcon } from "./icons";

type Step = "identifier" | "code";

/** Browser sign-in is only available when running inside the Electron shell. */
const browserSignIn =
  typeof window !== "undefined" ? window.codel00p?.auth : undefined;

/** Pulls a human-readable message out of a Clerk API error, with a fallback. */
function clerkErrorMessage(error: unknown, fallback: string): string {
  if (error && typeof error === "object" && "errors" in error) {
    const first = (error as { errors?: Array<{ longMessage?: string; message?: string }> })
      .errors?.[0];
    if (first?.longMessage) return first.longMessage;
    if (first?.message) return first.message;
  }
  return fallback;
}

export function LoginScreen() {
  const { isLoaded, signIn, setActive } = useSignIn();

  const [step, setStep] = useState<Step>("identifier");
  const [email, setEmail] = useState("");
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [browserBusy, setBrowserBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function continueInBrowser() {
    if (!isLoaded || browserBusy || !browserSignIn) return;
    setError(null);
    setBrowserBusy(true);
    try {
      const result = await browserSignIn.signInWithBrowser();
      if (result.error || !result.ticket) {
        throw new Error(result.error ?? "No sign-in ticket was returned.");
      }
      // Exchange the one-time ticket minted by the web app for a session.
      const attempt = await signIn.create({
        strategy: "ticket",
        ticket: result.ticket
      });
      if (attempt.status === "complete") {
        await setActive({ session: attempt.createdSessionId });
      } else {
        setError("Browser sign-in didn't complete. Try again.");
      }
    } catch (err) {
      setError(clerkErrorMessage(err, "Browser sign-in failed. Try again."));
    } finally {
      setBrowserBusy(false);
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
    <main className="relative grid h-full grid-cols-1 overflow-hidden pt-9 lg:grid-cols-[1.05fr_1fr]">
      <GlowBackground />

      <BrandPanel />

      {/* Form column */}
      <section className="relative flex items-center justify-center px-6 py-12 sm:px-10">
        <div className="rise w-full max-w-sm">
          <div className="mb-8 lg:hidden">
            <LoopMark className="size-9" />
          </div>

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
              ? browserSignIn
                ? "Sign in securely in your browser, or use a one-time email code."
                : "Sign in with a one-time email code."
              : `We sent a 6-digit code to ${email}.`}
          </p>

          {step === "identifier" ? (
            <>
              {browserSignIn ? (
                <div className="mt-8 flex flex-col gap-4">
                  <Button
                    type="button"
                    size="lg"
                    disabled={browserBusy}
                    onClick={continueInBrowser}
                    className="w-full justify-center gap-2.5 rounded-full"
                  >
                    {browserBusy ? (
                      <SpinnerIcon className="size-4" />
                    ) : (
                      <>
                        <BrowserIcon className="size-[18px]" />
                        Continue in browser
                      </>
                    )}
                  </Button>
                  <div className="flex items-center gap-3 text-muted-foreground/60">
                    <span className="h-px flex-1 bg-border" />
                    <span className="label text-[0.6rem]">or email code</span>
                    <span className="h-px flex-1 bg-border" />
                  </div>
                </div>
              ) : null}

              <form
                onSubmit={submitEmail}
                className={cn(
                  "flex flex-col gap-3",
                  browserSignIn ? "mt-4" : "mt-8"
                )}
              >
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

      <div className="flex items-center gap-3">
        <LoopMark className="size-9" />
        <span className="label text-muted-foreground">codel00p · desktop</span>
      </div>

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
          Supervise agents, review memory, and inspect project knowledge from
          one polished desktop workspace.
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
