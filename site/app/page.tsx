/* eslint-disable @next/next/no-img-element */

import { GitHubIcon, repoUrl, SiteFooter, SiteHeader } from "./site-chrome";

const demoUrl = "https://www.youtube.com/watch?v=HjOfFSyM-Mc";

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
    detail: "Landmarks drive the avatar.",
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
      <SiteHeader />

      <section className="hero shell" id="demo">
        <div className="hero-copy-block">
          <p className="eyebrow">Local camera motion capture</p>
          <h1>Live as<br />your avatar.</h1>
          <p className="hero-copy">
            Pocket Live turns your face, body, and hand movement into one live VRM
            performance. Everything runs on your Mac.
          </p>
          <div className="actions">
            <a className="button button-primary" href={repoUrl} target="_blank" rel="noreferrer">
              <span aria-hidden="true">↗</span> Build from source
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
          <a
            className="demo-video-link"
            href={demoUrl}
            target="_blank"
            rel="noreferrer"
            aria-label="Watch the full Pocket Live demo on YouTube"
          >
            <video
              src="/media/pocket-live-demo.mp4"
              poster="/media/pocket-live-demo-poster.jpg"
              autoPlay
              muted
              loop
              playsInline
              preload="metadata"
            />
            <span>Watch on YouTube ↗</span>
          </a>
          <div className="demo-caption">
            <span>Camera input → live VRM</span>
            <a href={demoUrl} target="_blank" rel="noreferrer">YouTube ↗</a>
          </div>
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
            <a className="button button-primary" href={repoUrl} target="_blank" rel="noreferrer">
              <span aria-hidden="true">↗</span> Build from source
            </a>
            <a className="button button-glass" href={repoUrl} target="_blank" rel="noreferrer">
              <GitHubIcon /> Star on GitHub
            </a>
          </div>
        </div>
      </section>

      <SiteFooter />
    </main>
  );
}
