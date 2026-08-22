import type { Metadata } from "next";
import { Geist, Geist_Mono } from "next/font/google";
import { headers } from "next/headers";
import "./globals.css";

const geistSans = Geist({ variable: "--font-geist-sans", subsets: ["latin"] });
const geistMono = Geist_Mono({ variable: "--font-geist-mono", subsets: ["latin"] });

export async function generateMetadata(): Promise<Metadata> {
  const requestHeaders = await headers();
  const host = (requestHeaders.get("x-forwarded-host") ?? requestHeaders.get("host") ?? "localhost:3000")
    .split(",")[0]
    .trim();
  const protocol = requestHeaders.get("x-forwarded-proto")?.split(",")[0].trim()
    ?? (host.startsWith("localhost") ? "http" : "https");
  const origin = `${protocol}://${host}`;

  return {
    metadataBase: new URL(origin),
    title: "Pocket Live — Local camera-to-VRM motion capture",
    description: "Turn face, body, and hand movement into a live VRM performance—locally, privately, and in real time on macOS.",
    alternates: { canonical: "/" },
    icons: { icon: "/favicon.png", shortcut: "/favicon.png" },
    openGraph: {
      title: "Pocket Live — Your camera in. Your character out.",
      description: "Fully local face, body, and hand tracking for real-time VRM live streams.",
      type: "website",
      url: origin,
      images: [{ url: `${origin}/og-v2.png`, width: 1731, height: 909, alt: "Pocket Live — Live as your avatar" }],
    },
    twitter: {
      card: "summary_large_image",
      title: "Pocket Live — Local motion capture",
      description: "Your camera in. Your character out. No cloud required.",
      images: [`${origin}/og-v2.png`],
    },
  };
}

export default function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en">
      <body className={`${geistSans.variable} ${geistMono.variable}`}>{children}</body>
    </html>
  );
}
