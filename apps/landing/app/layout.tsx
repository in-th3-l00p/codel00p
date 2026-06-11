import type { Metadata } from "next";
import { Pinyon_Script, Space_Grotesk, Geist_Mono } from "next/font/google";
import "./globals.css";

const sans = Space_Grotesk({
  variable: "--font-sans",
  subsets: ["latin"],
  display: "swap",
});

const hand = Pinyon_Script({
  variable: "--font-hand",
  subsets: ["latin"],
  weight: "400",
  display: "swap",
});

const mono = Geist_Mono({
  variable: "--font-mono",
  subsets: ["latin"],
  display: "swap",
});

const title = "codel00p — the coding agent that remembers";
const description =
  "An open-source agentic coding platform built around durable, reviewed project memory. Your team's knowledge compounds as the work gets done.";

export const metadata: Metadata = {
  title,
  description,
  metadataBase: new URL("https://codel00p.dev"),
  openGraph: {
    title,
    description,
    type: "website",
  },
  twitter: {
    card: "summary_large_image",
    title,
    description,
  },
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html
      lang="en"
      className={`dark ${sans.variable} ${hand.variable} ${mono.variable} h-full antialiased`}
    >
      <body className="min-h-full flex flex-col bg-background text-foreground">
        {children}
      </body>
    </html>
  );
}
