<script lang="ts">
  const APP_PORT = 3002;
  const INSTALL_URL = `${window.location.protocol}//${window.location.hostname}:${APP_PORT}/app/install`;

  let installing = false;

  function startInstall() {
    installing = true;
    window.location.href = INSTALL_URL;
  }
</script>

<svelte:head>
  <title>lopi — Connect your repo</title>
</svelte:head>

<main>
  <section class="hero">
    <h1>⛵ Connect your GitHub repo</h1>
    <p class="sub">
      lopi watches your repo, runs tests, finds gaps, and queues fix tasks — automatically.
      Connect in under 5 minutes.
    </p>
  </section>

  <section class="steps">
    <div class="step">
      <span class="num">1</span>
      <div>
        <strong>Install the GitHub App</strong>
        <p>Grant lopi read access to your repo. You can revoke at any time from GitHub Settings.</p>
      </div>
    </div>
    <div class="step">
      <span class="num">2</span>
      <div>
        <strong>Run lopi spec --save</strong>
        <p>Extract your test suite as a spec surface — the ground truth for what your repo claims to do.</p>
      </div>
    </div>
    <div class="step">
      <span class="num">3</span>
      <div>
        <strong>Start lopi watch-gap-fill</strong>
        <p>The continuous loop runs tests, finds failures, and queues fix tasks every hour.</p>
      </div>
    </div>
  </section>

  <section class="cta">
    <button class="install-btn" on:click={startInstall} disabled={installing}>
      {installing ? 'Redirecting to GitHub…' : '🔗 Install GitHub App'}
    </button>
    <p class="note">
      Already installed? Run <code>lopi sail</code> and visit the
      <a href="/">Forge dashboard</a>.
    </p>
  </section>

  <section class="pricing">
    <h2>Plans</h2>
    <div class="plans">
      <div class="plan">
        <div class="price">$299<span>/mo</span></div>
        <div class="name">Starter</div>
        <ul>
          <li>1 repo</li>
          <li>KCQF quality gate</li>
          <li>Weekly quality report</li>
          <li>Human reviews all changes</li>
        </ul>
      </div>
      <div class="plan featured">
        <div class="price">$999<span>/mo</span></div>
        <div class="name">Growth</div>
        <ul>
          <li>Up to 10 repos</li>
          <li>Continuous gap-fill loop</li>
          <li>Issue triage + auto-queue</li>
          <li>Trust calibration</li>
        </ul>
      </div>
      <div class="plan">
        <div class="price">$4,999<span>/mo</span></div>
        <div class="name">Enterprise</div>
        <ul>
          <li>Unlimited repos</li>
          <li>Full self-evolving loop</li>
          <li>squash compliance certs</li>
          <li>SLA: 99.9% uptime</li>
        </ul>
      </div>
    </div>
  </section>
</main>

<style>
  main {
    max-width: 860px;
    margin: 0 auto;
    padding: 56px 24px 96px;
    font-family: 'Syne', sans-serif;
    color: #9898b0;
  }

  .hero {
    text-align: center;
    margin-bottom: 56px;
  }

  h1 {
    font-size: 48px;
    font-weight: 800;
    color: #e8e8f2;
    letter-spacing: -0.03em;
    margin-bottom: 16px;
  }

  .sub {
    font-size: 18px;
    max-width: 600px;
    margin: 0 auto;
    line-height: 1.6;
  }

  .steps {
    display: flex;
    flex-direction: column;
    gap: 24px;
    margin-bottom: 48px;
  }

  .step {
    display: flex;
    gap: 20px;
    align-items: flex-start;
    background: #0e0e12;
    border: 1px solid #252530;
    border-radius: 8px;
    padding: 20px 24px;
  }

  .num {
    font-size: 24px;
    font-weight: 800;
    color: #22d3ee;
    min-width: 32px;
  }

  .step strong {
    color: #e8e8f2;
    font-size: 16px;
    display: block;
    margin-bottom: 4px;
  }

  .step p {
    margin: 0;
    font-size: 14px;
    line-height: 1.5;
  }

  .cta {
    text-align: center;
    margin-bottom: 64px;
  }

  .install-btn {
    background: #22d3ee;
    color: #08080a;
    font-weight: 700;
    font-size: 16px;
    border: none;
    border-radius: 6px;
    padding: 14px 32px;
    cursor: pointer;
    transition: opacity 0.15s;
  }

  .install-btn:hover:not(:disabled) {
    opacity: 0.85;
  }

  .install-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .note {
    margin-top: 16px;
    font-size: 13px;
  }

  .note a {
    color: #22d3ee;
    text-decoration: none;
  }

  code {
    background: #1a1a22;
    padding: 2px 6px;
    border-radius: 3px;
    font-family: 'Fira Code', monospace;
    font-size: 12px;
    color: #f59e0b;
  }

  h2 {
    font-size: 24px;
    font-weight: 700;
    color: #e8e8f2;
    text-align: center;
    margin-bottom: 24px;
  }

  .plans {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 16px;
  }

  .plan {
    background: #0e0e12;
    border: 1px solid #252530;
    border-radius: 8px;
    padding: 24px;
  }

  .plan.featured {
    border-color: #22d3ee;
  }

  .price {
    font-size: 32px;
    font-weight: 800;
    color: #e8e8f2;
    margin-bottom: 4px;
  }

  .price span {
    font-size: 14px;
    color: #48485c;
    font-weight: 400;
  }

  .name {
    font-size: 12px;
    letter-spacing: 0.15em;
    text-transform: uppercase;
    color: #48485c;
    margin-bottom: 16px;
  }

  .plan ul {
    list-style: none;
    padding: 0;
    margin: 0;
    font-size: 13px;
    line-height: 2;
  }

  .plan li::before {
    content: '✓ ';
    color: #10b981;
  }
</style>
