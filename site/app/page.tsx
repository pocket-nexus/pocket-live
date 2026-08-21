/* eslint-disable @next/next/no-img-element */

const repoUrl = "https://github.com/dozycat/pocket-live";

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

export default function Home() {
  return (
    <main id="top">
      <header className="site-header shell">
        <a className="brand" href="#top" aria-label="Pocket Live home">
          <i />
          <span>POCKET LIVE</span>
        </a>
        <nav aria-label="Main navigation">
          <a href="#demo">Demo</a>
          <a href="#why">Why Pocket Live</a>
        </nav>
        <a className="source-link" href={repoUrl} target="_blank" rel="noreferrer">
          GitHub <span aria-hidden="true">↗</span>
        </a>
      </header>

      <section className="hero shell">
        <p className="eyebrow">Local camera motion capture for macOS</p>
        <h1>Live as your avatar.</h1>
        <p className="hero-copy">
          Pocket Live turns face, body, and hand movement from your camera into a
          live VRM character. Everything runs on your Mac.
        </p>
        <div className="actions">
          <a className="button button-primary" href="#demo">Watch the demo</a>
          <a className="button button-secondary" href={repoUrl} target="_blank" rel="noreferrer">
            View source
          </a>
        </div>

        <figure className="hero-visual">
          <img
            src="/media/pocket-live-stage.png"
            alt="Pocket Live rendering a VRM avatar in a Japanese station scene"
            fetchPriority="high"
          />
          <figcaption>
            <span><i /> LIVE OUTPUT</span>
            <span>1080P · 60 FPS</span>
          </figcaption>
        </figure>
      </section>

      <section className="proof" aria-label="Pocket Live highlights">
        <div className="shell proof-grid">
          <div><strong>Face + body + hands</strong><span>One camera</span></div>
          <div><strong>Runs on your Mac</strong><span>No cloud tracking</span></div>
          <div><strong>Avatar-only output</strong><span>Ready for OBS</span></div>
        </div>
      </section>

      <section className="demo section shell" id="demo">
        <div className="section-copy">
          <p className="eyebrow">Real camera. Real capture.</p>
          <h2>See it move.</h2>
          <p>
            This short demo shows the actual build following a live camera feed—no
            pre-recorded animation.
          </p>
        </div>

        <div className="demo-frame">
          <iframe
            src="https://www.youtube-nocookie.com/embed/HjOfFSyM-Mc?rel=0&playsinline=1"
            title="Pocket Live camera motion-capture demo"
            loading="lazy"
            allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share"
            referrerPolicy="strict-origin-when-cross-origin"
            allowFullScreen
          />
          <a href="/media/pocket-live-demo.mp4">Use the lightweight local video instead</a>
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
          </figure>
        </div>
      </section>

      <section className="pipeline shell" aria-label="Pocket Live processing pipeline">
        <span>CAMERA</span><i>→</i>
        <span>APPLE VISION + MEDIAPIPE</span><i>→</i>
        <span>VRM</span><i>→</i>
        <span>OBS</span>
      </section>

      <section className="cta shell">
        <p className="eyebrow">Open source</p>
        <h2>Move naturally.<br />Stream as yourself.</h2>
        <a className="button button-primary" href={repoUrl} target="_blank" rel="noreferrer">
          Get Pocket Live on GitHub
        </a>
      </section>

      <footer className="site-footer shell">
        <a className="brand" href="#top"><i /><span>POCKET LIVE</span></a>
        <p>Local camera-to-VRM motion capture.</p>
        <span>macOS · Apple Silicon</span>
      </footer>
    </main>
  );
}
