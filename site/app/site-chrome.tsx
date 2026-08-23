import Link from "next/link";

const repoUrl = "https://github.com/pocket-stack/pocket-live";

export function GitHubIcon() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      <path d="M12 .7a11.5 11.5 0 0 0-3.64 22.4c.58.1.79-.25.79-.56v-2.02c-3.23.7-3.91-1.37-3.91-1.37-.53-1.34-1.29-1.7-1.29-1.7-1.05-.72.08-.71.08-.71 1.17.08 1.78 1.2 1.78 1.2 1.04 1.77 2.72 1.26 3.38.96.1-.75.4-1.26.74-1.55-2.58-.29-5.29-1.29-5.29-5.69 0-1.26.45-2.29 1.2-3.1-.12-.3-.52-1.48.12-3.06 0 0 .97-.31 3.17 1.18a10.98 10.98 0 0 1 5.77 0c2.2-1.49 3.17-1.18 3.17-1.18.64 1.58.24 2.76.12 3.06.75.81 1.2 1.84 1.2 3.1 0 4.41-2.72 5.39-5.3 5.68.42.36.79 1.07.79 2.16v3.2c0 .31.21.67.8.56A11.5 11.5 0 0 0 12 .7Z" />
    </svg>
  );
}

function LiveMark() {
  return (
    <span className="live-mark" aria-hidden="true">
      <span className="live-mark-dot" />
      <span className="live-mark-rec">REC</span>
    </span>
  );
}

function PocketJSMark() {
  return (
    <svg viewBox="0 0 32 32" aria-hidden="true" focusable="false">
      <defs>
        <linearGradient id="pocketjs-edge" x1="4" y1="4" x2="28" y2="28" gradientUnits="userSpaceOnUse">
          <stop offset="0" stopColor="#eef6ff" />
          <stop offset=".38" stopColor="#b7c8e2" />
          <stop offset=".58" stopColor="#7487a0" />
          <stop offset=".78" stopColor="#aec0d6" />
          <stop offset="1" stopColor="#dbe8f6" />
        </linearGradient>
        <linearGradient id="pocketjs-lens" x1="8" y1="13" x2="12" y2="19" gradientUnits="userSpaceOnUse">
          <stop offset="0" stopColor="#baff75" />
          <stop offset="1" stopColor="#4f7800" />
        </linearGradient>
        <linearGradient id="pocketjs-bar" x1="16" y1="12" x2="24" y2="20" gradientUnits="userSpaceOnUse">
          <stop offset="0" stopColor="#d7e3f1" />
          <stop offset="1" stopColor="#71849d" />
        </linearGradient>
      </defs>
      <rect x="2" y="6" width="28" height="20" rx="6" fill="none" stroke="url(#pocketjs-edge)" strokeWidth="2.6" strokeLinejoin="round" />
      <circle cx="10" cy="16" r="3.1" fill="url(#pocketjs-lens)" />
      <rect x="16" y="12.6" width="10" height="2.2" rx="1.1" fill="url(#pocketjs-bar)" />
      <rect x="16" y="17.2" width="6.5" height="2.2" rx="1.1" fill="url(#pocketjs-bar)" />
    </svg>
  );
}

export function SiteHeader() {
  return (
    <header className="site-header shell">
      <Link className="brand" href="/#top" aria-label="Pocket Live home">
        <LiveMark />
        <span>POCKET LIVE</span>
      </Link>
      <nav aria-label="Main navigation">
        <Link href="/#signal">How it works</Link>
        <Link href="/blog">Blog</Link>
      </nav>
      <a className="github-button github-button-compact" href={repoUrl} target="_blank" rel="noreferrer">
        <GitHubIcon />
        <span>Star on GitHub</span>
      </a>
    </header>
  );
}

export function SiteFooter() {
  return (
    <footer className="site-footer shell">
      <Link className="brand" href="/#top" aria-label="Pocket Live home">
        <LiveMark />
        <span>POCKET LIVE</span>
      </Link>
      <div className="footer-links">
        <a href="https://pocketlab.build/">A Pocket Lab Project</a>
        <a className="powered-by" href="https://pocketjs.dev/" target="_blank" rel="noreferrer" aria-label="Powered by PocketJS">
          <span>Powered by</span>
          <PocketJSMark />
          <strong>PocketJS</strong>
        </a>
      </div>
    </footer>
  );
}

export { repoUrl };
