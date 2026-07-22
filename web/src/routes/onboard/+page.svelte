<script lang="ts">
  const APP_PORT = 3002;
  const INSTALL_URL = `${window.location.protocol}//${window.location.hostname}:${APP_PORT}/app/install`;

  let installing = false;

  function startInstall() {
    installing = true;
    window.location.href = INSTALL_URL;
  }

  const STEPS = [
    {
      title: 'Install the GitHub App',
      detail: 'Grant lopi read access to your repo. You can revoke at any time from GitHub Settings.'
    },
    {
      title: 'Run lopi spec --save',
      detail: 'Extract your test suite as a spec surface — the ground truth for what your repo claims to do.'
    },
    {
      title: 'Start lopi watch-gap-fill',
      detail: 'The continuous loop runs tests, finds failures, and queues fix tasks every hour.'
    }
  ];

  const PLANS = [
    {
      price: '$299',
      name: 'Starter',
      featured: false,
      features: ['1 repo', 'KCQF quality gate', 'Weekly quality report', 'Human reviews all changes']
    },
    {
      price: '$999',
      name: 'Growth',
      featured: true,
      features: ['Up to 10 repos', 'Continuous gap-fill loop', 'Issue triage + auto-queue', 'Trust calibration']
    },
    {
      price: '$4,999',
      name: 'Enterprise',
      featured: false,
      features: ['Unlimited repos', 'Full self-evolving loop', 'Squash compliance certs', 'SLA: 99.9% uptime']
    }
  ];
</script>

<svelte:head>
  <title>lopi — Connect your repo</title>
</svelte:head>

<div class="max-w-3xl mx-auto px-6 py-16 space-y-14">
  <!-- Hero -->
  <section class="text-center space-y-4">
    <h1 class="font-display text-4xl tracking-tight">⛵ Connect your GitHub repo</h1>
    <p class="font-mono text-sm opacity-60 max-w-xl mx-auto leading-relaxed">
      lopi watches your repo, runs tests, finds gaps, and queues fix tasks — automatically.
      Connect in under 5 minutes.
    </p>
  </section>

  <!-- Steps -->
  <section class="space-y-3">
    {#each STEPS as step, i}
      <div class="flex gap-5 items-start rounded-lg border border-white/5 bg-konjo-deep/60 backdrop-blur-sm px-5 py-4">
        <span class="font-display text-xl text-konjo-ice flex-shrink-0 w-8">{i + 1}</span>
        <div class="min-w-0">
          <div class="font-display text-sm font-bold">{step.title}</div>
          <p class="font-mono text-[11px] opacity-50 mt-1 leading-relaxed">{step.detail}</p>
        </div>
      </div>
    {/each}
  </section>

  <!-- CTA -->
  <section class="text-center space-y-3">
    <button
      type="button"
      on:click={startInstall}
      disabled={installing}
      class="press font-mono text-xs uppercase tracking-widest px-6 py-3 rounded-lg border border-konjo-ice/40 bg-konjo-ice/10 text-konjo-ice hover:bg-konjo-ice/20 disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
    >
      {installing ? 'redirecting to github…' : '🔗 install github app'}
    </button>
    <p class="font-mono text-[11px] opacity-40">
      Already installed? Run <code class="px-1.5 py-0.5 rounded bg-black/40 border border-white/10 text-konjo-sun">lopi sail</code>
      and visit <a href="/stacks" class="text-konjo-ice hover:underline">Loop Stacks</a>.
    </p>
  </section>

  <!-- Pricing -->
  <section class="space-y-5">
    <h2 class="font-display text-lg text-center">Plans</h2>
    <div class="grid grid-cols-1 sm:grid-cols-3 gap-3">
      {#each PLANS as plan}
        <div
          class="rounded-lg border bg-konjo-deep/60 backdrop-blur-sm p-5"
          class:border-konjo-ice={plan.featured}
          class:border-white-5={!plan.featured}
          style:border-color={plan.featured ? 'var(--konjo-ice)' : 'rgba(255,255,255,0.05)'}
        >
          <div class="font-display text-2xl">
            {plan.price}<span class="font-mono text-xs opacity-40">/mo</span>
          </div>
          <div class="font-mono text-[10px] uppercase tracking-widest opacity-40 mt-1 mb-4">
            {plan.name}
          </div>
          <ul class="space-y-1.5">
            {#each plan.features as f}
              <li class="font-mono text-[11px] opacity-70 flex items-start gap-1.5">
                <span class="text-konjo-jade">✓</span>{f}
              </li>
            {/each}
          </ul>
        </div>
      {/each}
    </div>
  </section>
</div>
