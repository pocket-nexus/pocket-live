import type { Metadata } from "next";
import Link from "next/link";
import { SiteFooter, SiteHeader } from "../../site-chrome";

export const metadata: Metadata = {
  title: "Why Pocket Live — Features, Live2D, VRM, and the local pipeline",
  description: "What Pocket Live does, how its local avatar pipeline works, and why it uses VRM for real-time face, body, and hand tracking.",
  alternates: { canonical: "/blog" },
};

const live2dDeformerUrl = "https://docs.live2d.com/en/cubism-editor-manual/deformer/";
const live2dParameterUrl = "https://docs.live2d.com/en/cubism-editor-manual/parameter/";
const live2dLicenseUrl = "https://www.live2d.com/en/sdk/license/";
const vrmFeaturesUrl = "https://vrm.dev/en/vrm/vrm_features/";
const vrmDevelopmentUrl = "https://vrm.dev/en/vrm/vrm_development/";
const pocketCharacterUrl = "https://pocketjs.dev/blog/pocket-character/";

export default function Live2DVsVrmArticle() {
  return (
    <main id="top">
      <SiteHeader />
      <article className="article shell">
        <header className="article-header">
          <Link className="article-back" href="/">← Pocket Live</Link>
          <p className="eyebrow">Product & architecture</p>
          <h1>Why Pocket Live.</h1>
          <p className="article-deck">What it does, why it stays local, and why a full-body performance pipeline led us to VRM.</p>
          <div className="article-meta">
            <time dateTime="2026-08-23">August 23, 2026</time>
            <span>9 min read</span>
          </div>
        </header>

        <div className="article-body">
          <p className="article-lead">
            Pocket Live turns one camera into a complete virtual performance. Face, body, and
            hand movement are tracked locally, stabilized, mapped onto a VRM avatar, and rendered
            as a clean scene ready for streaming—without sending camera frames to the cloud.
          </p>

          <h2>What Pocket Live does</h2>
          <div className="article-feature-grid">
            <article>
              <span>01</span>
              <h3>One camera</h3>
              <p>Face, head, upper body, arms, and hands contribute to one continuous performance.</p>
            </article>
            <article>
              <span>02</span>
              <h3>Fully local</h3>
              <p>Tracking and rendering run on your Mac. The camera feed never needs a cloud round trip.</p>
            </article>
            <article>
              <span>03</span>
              <h3>One avatar signal</h3>
              <p>Filtered landmarks drive humanoid bones, expressions, gaze, and secondary motion together.</p>
            </article>
            <article>
              <span>04</span>
              <h3>Stream-ready output</h3>
              <p>Only the composed avatar scene reaches OBS or your live platform.</p>
            </article>
          </div>

          <p>
            The product is deliberately narrower than a general animation suite. It owns the path
            from live camera motion to a stable character scene, so creators can spend their time
            performing rather than wiring together separate face, body, renderer, and output tools.
          </p>

          <h2>Live2D and VRM: two good solutions, two different abstractions</h2>
          <p>
            Live2D Cubism starts with layered artwork. ArtMeshes and deformers create the
            authored shapes for head turns, mouth movement, arms, hair, and clothing. Parameters
            such as Angle X or Mouth Open/Close interpolate between those shapes. The result can
            retain the exact linework and visual language of an illustrator. That is Live2D&apos;s
            defining strength, not a limitation. The mechanics are described in Live2D&apos;s official
            guides to <a href={live2dDeformerUrl} target="_blank" rel="noreferrer">deformers</a> and
            <a href={live2dParameterUrl} target="_blank" rel="noreferrer"> parameters</a>.
          </p>
          <p>
            VRM starts from a different premise. It packages a humanoid 3D avatar and standardizes
            how applications find its bones, expressions, gaze, materials, secondary motion, and
            metadata. The format is designed for runtime loading and general-purpose motion capture,
            with the avatar data carried in one file. The VRM Consortium documents these
            <a href={vrmFeaturesUrl} target="_blank" rel="noreferrer"> avatar operations</a> and
            <a href={vrmDevelopmentUrl} target="_blank" rel="noreferrer"> humanoid conventions</a>.
          </p>

          <div className="comparison-table-wrap">
            <table className="comparison-table">
              <thead>
                <tr>
                  <th scope="col">Decision</th>
                  <th scope="col">Live2D</th>
                  <th scope="col">VRM</th>
                </tr>
              </thead>
              <tbody>
                <tr>
                  <th scope="row">Visual language</th>
                  <td>Authored 2D illustration and mesh deformation</td>
                  <td>Spatial 3D character, materials, lighting, and camera</td>
                </tr>
                <tr>
                  <th scope="row">Motion interface</th>
                  <td>Model-specific parameters and keyforms</td>
                  <td>Standard humanoid bones, expressions, and gaze</td>
                </tr>
                <tr>
                  <th scope="row">Body and hands</th>
                  <td>Possible when the model is authored with the required parameters</td>
                  <td>Skeletal pose maps directly onto articulated limbs and fingers</td>
                </tr>
                <tr>
                  <th scope="row">Viewpoint</th>
                  <td>Best inside the angles and forms prepared by the artist</td>
                  <td>Free 3D camera movement and scene composition</td>
                </tr>
                <tr>
                  <th scope="row">Runtime integration</th>
                  <td>Cubism model data rendered through the Cubism ecosystem</td>
                  <td>Platform-independent avatar data built on glTF conventions</td>
                </tr>
              </tbody>
            </table>
          </div>

          <h2>Why Pocket Live uses VRM</h2>
          <p>
            Pocket Live begins with spatial observations: face rotation and expressions, shoulders,
            elbows, wrists, hips, and hand landmarks. A VRM humanoid gives those observations stable
            semantic targets. The solver can address leftUpperArm, leftLowerArm, hand, fingers, head,
            and expression channels without inventing a new control vocabulary for every avatar.
          </p>

          <div className="article-callout">
            <p className="eyebrow">The Pocket Live path</p>
            <p>Camera → local landmarks → filtered pose → VRM humanoid → native render → clean stream</p>
          </div>

          <p>
            This also keeps the whole scene coherent. The avatar, camera, lighting, background, and
            effects share one 3D renderer. A creator can change the camera or stage without asking the
            model artist to draw another view. And because the tracking and rendering stay on the Mac,
            only the final avatar scene needs to reach streaming software.
          </p>

          <h2>What VRM does not solve for us</h2>
          <p>
            A standard skeleton is not a motion-quality button. Webcam landmarks still become noisy
            when hands cross the torso, limbs leave frame, or depth is ambiguous. Pocket Live still
            needs confidence gating, temporal filtering, joint limits, inverse kinematics, collision
            guardrails, and graceful recovery when tracking disappears. Rig quality matters too: a
            technically valid avatar can still deform poorly around shoulders or elbows.
          </p>
          <p>
            That distinction matters. VRM reduces the amount of model-specific integration; it does
            not remove the work required to make continuous human motion look natural.
          </p>

          <h2>When Live2D is the better choice</h2>
          <p>
            Choose Live2D when the character&apos;s identity lives in a particular drawing, when a mostly
            frontal performance is the goal, or when carefully authored exaggeration matters more than
            free 3D movement. A strong Live2D rig can feel more expressive than an average 3D model
            because every deformation was composed for that character.
          </p>
          <p>
            Teams embedding Cubism should also review the official
            <a href={live2dLicenseUrl} target="_blank" rel="noreferrer"> SDK publication terms</a> for
            their product and business model. The terms distinguish development, publication, company
            size, and expandable applications, so there is no useful one-line licensing answer.
          </p>

          <h2>When Pocket Live is the better fit</h2>
          <p>
            Pocket Live is aimed at creators who want one camera to drive face, body, and hands; who
            want a 3D avatar that can move between scenes; and who prefer a local pipeline they can
            inspect and extend. VRM is the interchange layer. PocketJS is the execution layer.
          </p>
          <p>
            PocketJS already supplies native VRM rendering, morph targets, animation retargeting, and
            spring-bone physics in a compact process. Its
            <a href={pocketCharacterUrl} target="_blank" rel="noreferrer"> Pocket Character engineering note</a>
            explains the renderer and its measured native architecture. Pocket Live adds the live
            camera, tracking, stabilization, character plugins, backgrounds, and stream-ready output.
          </p>

          <h2>What the full local pipeline actually costs</h2>
          <p>
            An idle character benchmark does not represent a live product. We measured Pocket Live
            with the camera, face, pose, and hand inference, VRM mapping, virtual background, and
            1920×1080 window rendering all active. OBS was deliberately excluded so this is a
            measurement of Pocket Live itself, not the streaming encoder.
          </p>

          <div className="benchmark-metrics" aria-label="Pocket Live full-pipeline benchmark summary">
            <article>
              <strong>43.1%</strong>
              <span>median total CPU</span>
              <small>41.0–45.3% observed</small>
            </article>
            <article>
              <strong>638 MiB</strong>
              <span>median summed RSS</span>
              <small>637–791 MiB observed</small>
            </article>
            <article>
              <strong>30 fps</strong>
              <span>camera capture</span>
              <small>1920×1080 input</small>
            </article>
            <article>
              <strong>15 fps</strong>
              <span>landmark inference</span>
              <small>640×360 working frame</small>
            </article>
          </div>

          <div className="comparison-table-wrap benchmark-table-wrap">
            <table className="comparison-table benchmark-table">
              <thead>
                <tr>
                  <th scope="col">Runtime process</th>
                  <th scope="col">Median CPU</th>
                  <th scope="col">Median RSS</th>
                </tr>
              </thead>
              <tbody>
                <tr>
                  <th scope="row">PocketJS / VRM render host</th>
                  <td>7.5%</td>
                  <td>127 MiB</td>
                </tr>
                <tr>
                  <th scope="row">Apple Vision camera bridge</th>
                  <td>4.8%</td>
                  <td>209 MiB</td>
                </tr>
                <tr>
                  <th scope="row">MediaPipe face + pose + hands</th>
                  <td>31.0%</td>
                  <td>302 MiB</td>
                </tr>
                <tr>
                  <th scope="row">Complete Pocket Live pipeline</th>
                  <td>43.1%</td>
                  <td>638 MiB</td>
                </tr>
              </tbody>
            </table>
          </div>

          <p className="benchmark-method">
            <strong>Test setup.</strong> MacBook Pro with Apple M5 Max, 18 CPU cores, 48 GB memory,
            and macOS 26.6.2; Pocket Live&apos;s default 27 MB VRM avatar and comic background; camera
            tracking enabled. We allowed 25 seconds for warm-up, then collected 13 process-tree
            samples at five-second intervals. CPU uses the macOS <code>ps</code> convention, where
            100% means one fully occupied logical core. Memory is the sum of resident set size for
            the three product processes and can count shared pages more than once. Per-process
            medians are calculated independently, so rounded rows need not add exactly to the total.
          </p>
          <p>
            The useful result is not just the total: landmark inference is the dominant CPU cost.
            The PocketJS render host—including the VRM avatar, animation, expressions, background,
            and native window—used a 7.5% median CPU in this run. That is why Pocket Live keeps
            PocketJS as a compact execution layer and spends its performance budget on motion
            quality. This is one reproducible machine-and-scene snapshot, not a universal guarantee;
            avatar complexity, camera hardware, and tracking settings will move the numbers.
          </p>

          <h2>The decision in one sentence</h2>
          <p>
            Live2D is the right tool when you want to animate a drawing; Pocket Live chose VRM because
            it wants to translate a person&apos;s spatial performance into a portable, fully rendered 3D
            avatar—locally and in real time.
          </p>

          <section className="article-sources" aria-labelledby="sources-title">
            <h2 id="sources-title">Primary sources</h2>
            <ul>
              <li><a href={live2dDeformerUrl} target="_blank" rel="noreferrer">Live2D Cubism: About Deformers</a></li>
              <li><a href={live2dParameterUrl} target="_blank" rel="noreferrer">Live2D Cubism: About Parameters</a></li>
              <li><a href={live2dLicenseUrl} target="_blank" rel="noreferrer">Live2D Cubism: SDK Release License</a></li>
              <li><a href={vrmFeaturesUrl} target="_blank" rel="noreferrer">VRM Consortium: Features and contents of VRM</a></li>
              <li><a href={vrmDevelopmentUrl} target="_blank" rel="noreferrer">VRM Consortium: VRM development</a></li>
              <li><a href={pocketCharacterUrl} target="_blank" rel="noreferrer">PocketJS: Pocket Character engineering note</a></li>
            </ul>
          </section>
        </div>
      </article>
      <SiteFooter />
    </main>
  );
}
