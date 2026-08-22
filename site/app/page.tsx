/* eslint-disable @next/next/no-img-element */

const repoUrl = "https://github.com/pocket-stack/pocket-live";
const downloadUrl = `${repoUrl}/releases`;

const benefits = [
  {
    title: "Natural movement",
    copy: "Face, body, and hands move the same VRM character together.",
  },
  {
    title: "Private by design",
    copy: "Camera frames stay on your Mac. Only the avatar reaches your stream.",
  },
  {
    title: "Ready for OBS",
    copy: "Render a clean, avatar-only scene at up to 1080p and 60 fps.",
  },
];

const signalStages = [
  {
    number: "01",
    label: "Camera",
    title: "One local feed",
    detail: "Your camera stays on device.",
    visual: "camera",
  },
  {
    number: "02",
    label: "Track",
    title: "Apple Vision + MediaPipe",
    detail: "Face, hands, and pose resolve together.",
    visual: "tracking",
  },
  {
    number: "03",
    label: "Drive",
    title: "Live VRM motion",
    detail: "Landmarks become natural character motion.",
    visual: "avatar",
  },
  {
    number: "04",
    label: "Stream",
    title: "Clean OBS output",
    detail: "Only your avatar reaches the scene.",
    visual: "output",
  },
];

function GitHubIcon() {
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

function SignalVisual({ kind }: { kind: string }) {
  if (kind === "camera") {
    return (
      <div className="signal-camera" aria-hidden="true">
        <span className="camera-lens"><i /></span>
        <span className="camera-private">LOCAL</span>
      </div>
    );
  }

  if (kind === "tracking") {
    return (
      <div className="signal-body" aria-hidden="true">
        <i className="joint joint-head" />
        <i className="joint joint-left-hand" />
        <i className="joint joint-right-hand" />
        <i className="joint joint-core" />
        <i className="joint joint-left-foot" />
        <i className="joint joint-right-foot" />
        <span className="bone bone-arms" />
        <span className="bone bone-spine" />
        <span className="bone bone-legs-left" />
        <span className="bone bone-legs-right" />
        <b>42 landmarks</b>
      </div>
    );
  }

  if (kind === "avatar") {
    return (
      <div className="signal-avatar" aria-hidden="true">
        <img src="/media/pocket-live-transparent.png" alt="" loading="lazy" />
        <span>VRM</span>
      </div>
    );
  }

  return (
    <div className="signal-output" aria-hidden="true">
      <span className="output-window"><i /><i /><i /></span>
      <span className="output-status"><i /> LIVE</span>
      <b>OBS</b>
    </div>
  );
}

export default function Home() {
  return (
    <main id="top">
      <header className="site-header shell">
        <a className="brand" href="#top" aria-label="Pocket Live home">
          <LiveMark />
          <span>POCKET LIVE</span>
        </a>
        <nav aria-label="Main navigation">
          <a href="#why">Why Pocket Live</a>
          <a href="#signal">How it works</a>
        </nav>
        <a className="github-button github-button-compact" href={repoUrl} target="_blank" rel="noreferrer">
          <GitHubIcon />
          <span>Star on GitHub</span>
        </a>
      </header>

      <section className="hero shell" id="demo">
        <div className="hero-copy-block">
          <p className="eyebrow">Local camera motion capture</p>
          <h1>Live as<br />your avatar.</h1>
          <p className="hero-copy">
            Pocket Live turns your face, body, and hand movement into one live VRM
            performance. Everything runs on your Mac.
          </p>
          <div className="actions">
            <a className="button button-primary" href={downloadUrl} target="_blank" rel="noreferrer">
              <span aria-hidden="true">↓</span> Download
            </a>
            <a className="button button-secondary" href={repoUrl} target="_blank" rel="noreferrer">
              <GitHubIcon /> Star on GitHub
            </a>
          </div>
          <ul className="hero-facts" aria-label="Pocket Live highlights">
            <li><strong>Face + body + hands</strong><span>One camera</span></li>
            <li><strong>Fully local</strong><span>No cloud tracking</span></li>
            <li><strong>OBS-ready</strong><span>Clean avatar output</span></li>
          </ul>
        </div>

        <div className="hero-demo">
          <div className="demo-topline">
            <span><i /> REAL CAPTURE</span>
            <span>18 SEC</span>
          </div>
          <iframe
            src="https://www.youtube-nocookie.com/embed/HjOfFSyM-Mc?rel=0&playsinline=1"
            title="Pocket Live camera motion-capture demo"
            allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share"
            referrerPolicy="strict-origin-when-cross-origin"
            allowFullScreen
          />
          <div className="demo-caption">
            <span>Camera input → live VRM</span>
            <a href="/media/pocket-live-demo.mp4">MP4 ↗</a>
          </div>
        </div>
      </section>

      <section className="why section" id="why">
        <div className="shell why-layout">
          <div className="why-copy">
            <p className="eyebrow">Made for going live</p>
            <h2>One camera.<br />No cloud.</h2>
            <div className="benefit-list">
              {benefits.map((benefit, index) => (
                <article key={benefit.title}>
                  <span>0{index + 1}</span>
                  <div>
                    <h3>{benefit.title}</h3>
                    <p>{benefit.copy}</p>
                  </div>
                </article>
              ))}
            </div>
          </div>

          <figure className="tracking-visual">
            <img
              src="/media/pocket-live-tracking.png"
              alt="Pocket Live avatar responding to tracked arm movement"
              loading="lazy"
            />
            <figcaption><i /> LIVE BODY TRACKING</figcaption>
          </figure>
        </div>
      </section>

      <section className="signal-section shell" id="signal" aria-labelledby="signal-title">
        <div className="signal-heading">
          <div>
            <p className="eyebrow">One continuous signal</p>
            <h2 id="signal-title">From lens to live.</h2>
          </div>
          <p>Hover or focus each stage to follow your movement from a private camera frame to a clean stream.</p>
        </div>

        <ol className="signal-map">
          {signalStages.map((stage) => (
            <li className={`signal-card signal-card-${stage.visual}`} key={stage.number} tabIndex={0}>
              <div className="signal-meta"><span>{stage.number}</span><span>{stage.label}</span></div>
              <SignalVisual kind={stage.visual} />
              <div className="signal-copy">
                <h3>{stage.title}</h3>
                <p>{stage.detail}</p>
              </div>
            </li>
          ))}
        </ol>
      </section>

      <section className="cta-banner" aria-labelledby="cta-title">
        <img src="/media/pocket-live-stage.png" alt="" loading="lazy" />
        <div className="cta-shade" />
        <div className="cta-content shell">
          <p className="eyebrow">Open source motion capture</p>
          <h2 id="cta-title">Move naturally.<br />Stream as yourself.</h2>
          <div className="actions">
            <a className="button button-primary" href={downloadUrl} target="_blank" rel="noreferrer">
              <span aria-hidden="true">↓</span> Download
            </a>
            <a className="button button-glass" href={repoUrl} target="_blank" rel="noreferrer">
              <GitHubIcon /> Star on GitHub
            </a>
          </div>
        </div>
      </section>

      <footer className="site-footer shell">
        <a className="brand" href="#top" aria-label="Pocket Live home">
          <LiveMark />
          <span>POCKET LIVE</span>
        </a>
        <div className="footer-links">
          <a href="https://pocketlab.build/">A Pocket Lab Project</a>
          <a className="powered-by" href="https://pocketjs.dev/" target="_blank" rel="noreferrer" aria-label="Powered by PocketJS">
            <span>Powered by</span>
            <PocketJSMark />
            <strong>PocketJS</strong>
          </a>
        </div>
      </footer>
    </main>
  );
}
