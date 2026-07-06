<!--
  The Forge — lopi's centerpiece visualization.

  A perpetually morphing sphere of fire and ice, driven by the live cognitive
  state of an agent (or the aggregate of all agents).

  Custom GLSL fragment shader composes:
    1. Three octaves of simplex noise (Ashima) for surface texture
    2. A sinusoidal fire/ice domain boundary modulated by the noise field
    3. A view-direction Fresnel term tinted by the phase color (the aura)

  Live inputs:
    - pressure (0..1) — context window fill, drives turbulence + displacement
    - phaseColor (vec3)  — current agent phase, drives the aura tint
    - activity (0..1)  — tokens/sec normalized, drives pulse rate
-->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import * as THREE from 'three';
  import {
    EXCITE_DURATION_MS,
    smoothstep01,
    shakeAmplitude,
    spinMultiplier,
    exciteColor,
    shakes,
    type StimulusKind
  } from './excitement';
  import type { OrbSpecial } from './orbState';

  // ── Public props ────────────────────────────────────────────────────────────
  export let pressure: number = 0.4;          // 0..1 — context fill
  export let phaseColor: string = '#00d4ff';  // CSS hex
  export let activity: number = 0.5;          // 0..1 — generation intensity
  export let health: number = 0.85;           // 0..1 — recent success rate
  export let size: number = 320;              // px
  /**
   * Stimulus timestamp (ms). Bump to a newer value whenever the agent
   * receives a request — the orb reacts: a quick shake, then faster spin
   * and an orange ember glow that decays back to calm over ~2.5s.
   */
  export let stimulus: number = 0;
  /** What kind of stimulus — picks the reaction color + whether to shake. */
  export let stimulusKind: StimulusKind = 'request';

  // ── Living-orb motion params (ORB STATE MAP, see orbState.ts) ────────────────
  /** State hue; overrides phaseColor when set so the orb takes the status color. */
  export let glowColor: string = '';
  /** Spin rate, baseline 1; 0 stops (only honored under `special: hardStop`). */
  export let spinSpeed: number = 1;
  /** Pulse-frequency multiplier, baseline 1. */
  export let pulseRate: number = 1;
  /** Aura / rim brightness, ~0.2 (idle) … ~1.4 (success bloom). */
  export let glowIntensity: number = 0.85;
  /** Surface displacement intensity, 0..1, layered onto pressure. */
  export let turbulence: number = 0.3;
  /** A named motion flourish: kryptonite, hardStop, reverseSpin, stutter, … */
  export let special: OrbSpecial = 'none';

  /** Resolved orb hue — the state color when provided, else the phase color. */
  $: hue = glowColor || phaseColor;

  const reduceMotion =
    typeof window !== 'undefined' && window.matchMedia
      ? window.matchMedia('(prefers-reduced-motion: reduce)').matches
      : false;

  // Kryptonite envelope: bright jade halo pulses on arrival then settles.
  let krypto = 0;
  let lastSpecial: OrbSpecial = 'none';
  $: if (special === 'kryptonite' && lastSpecial !== 'kryptonite') {
    krypto = 1;
  }
  $: lastSpecial = special;

  // ── Internal state ──────────────────────────────────────────────────────────
  let container: HTMLDivElement;
  let renderer: THREE.WebGLRenderer | null = null;
  let scene: THREE.Scene;
  let camera: THREE.PerspectiveCamera;
  let mesh: THREE.Mesh;
  let material: THREE.ShaderMaterial;
  let frameId: number | null = null;
  let lastTime = 0;

  // Excitement envelope (0..1) — set to 1 on stimulus, decays in animate().
  let excite = 0;
  let lastStimulus = 0;

  /** Kick the excitement envelope — also callable directly by parents. */
  export function excited() {
    excite = 1;
  }

  $: if (stimulus > lastStimulus) {
    lastStimulus = stimulus;
    excite = 1;
  }

  // Reaction color follows the stimulus kind.
  $: if (material && stimulusKind) {
    const [r, g, b] = exciteColor(stimulusKind);
    (material.uniforms.uExciteColor.value as THREE.Vector3).set(r, g, b);
  }

  // ── Vertex shader ───────────────────────────────────────────────────────────
  // Displaces sphere vertices outward by a noise field; intensity scales with
  // pressure. The noise value is passed to the fragment shader for shading.
  const vertexShader = /* glsl */ `
    varying vec3 vPosition;
    varying vec3 vNormal;
    varying vec3 vWorldPosition;
    varying float vNoise;

    uniform float uTime;
    uniform float uPressure;
    uniform float uActivity;
    uniform float uTurbulence;

    // Ashima 3D simplex noise
    vec3 mod289(vec3 x) { return x - floor(x * (1.0 / 289.0)) * 289.0; }
    vec4 mod289(vec4 x) { return x - floor(x * (1.0 / 289.0)) * 289.0; }
    vec4 permute(vec4 x) { return mod289(((x*34.0)+1.0)*x); }
    vec4 taylorInvSqrt(vec4 r) { return 1.79284291400159 - 0.85373472095314 * r; }

    float snoise(vec3 v) {
      const vec2 C = vec2(1.0/6.0, 1.0/3.0);
      const vec4 D = vec4(0.0, 0.5, 1.0, 2.0);
      vec3 i  = floor(v + dot(v, C.yyy));
      vec3 x0 = v - i + dot(i, C.xxx);
      vec3 g = step(x0.yzx, x0.xyz);
      vec3 l = 1.0 - g;
      vec3 i1 = min(g.xyz, l.zxy);
      vec3 i2 = max(g.xyz, l.zxy);
      vec3 x1 = x0 - i1 + C.xxx;
      vec3 x2 = x0 - i2 + C.yyy;
      vec3 x3 = x0 - D.yyy;
      i = mod289(i);
      vec4 p = permute(permute(permute(
                 i.z + vec4(0.0, i1.z, i2.z, 1.0))
               + i.y + vec4(0.0, i1.y, i2.y, 1.0))
               + i.x + vec4(0.0, i1.x, i2.x, 1.0));
      float n_ = 0.142857142857;
      vec3 ns = n_ * D.wyz - D.xzx;
      vec4 j = p - 49.0 * floor(p * ns.z * ns.z);
      vec4 x_ = floor(j * ns.z);
      vec4 y_ = floor(j - 7.0 * x_);
      vec4 x = x_ *ns.x + ns.yyyy;
      vec4 y = y_ *ns.x + ns.yyyy;
      vec4 h = 1.0 - abs(x) - abs(y);
      vec4 b0 = vec4(x.xy, y.xy);
      vec4 b1 = vec4(x.zw, y.zw);
      vec4 s0 = floor(b0)*2.0 + 1.0;
      vec4 s1 = floor(b1)*2.0 + 1.0;
      vec4 sh = -step(h, vec4(0.0));
      vec4 a0 = b0.xzyw + s0.xzyw*sh.xxyy;
      vec4 a1 = b1.xzyw + s1.xzyw*sh.zzww;
      vec3 p0 = vec3(a0.xy, h.x);
      vec3 p1 = vec3(a0.zw, h.y);
      vec3 p2 = vec3(a1.xy, h.z);
      vec3 p3 = vec3(a1.zw, h.w);
      vec4 norm = taylorInvSqrt(vec4(dot(p0,p0), dot(p1,p1), dot(p2,p2), dot(p3,p3)));
      p0 *= norm.x; p1 *= norm.y; p2 *= norm.z; p3 *= norm.w;
      vec4 m = max(0.6 - vec4(dot(x0,x0), dot(x1,x1), dot(x2,x2), dot(x3,x3)), 0.0);
      m = m * m;
      return 42.0 * dot(m*m, vec4(dot(p0,x0), dot(p1,x1), dot(p2,x2), dot(p3,x3)));
    }

    void main() {
      vPosition = position;
      vNormal = normalize(normalMatrix * normal);

      // Slow drift + activity-driven turbulence
      float t = uTime * (0.25 + uActivity * 0.5);

      // Multi-octave displacement noise
      float n1 = snoise(position * 1.8 + vec3(t, 0.0, 0.0));
      float n2 = snoise(position * 4.5 + vec3(0.0, t * 1.3, 0.0));
      float noise = n1 * 0.7 + n2 * 0.3;

      // Pressure + state turbulence scale displacement amplitude
      // (0.05 calm → ~0.38 at full pressure + turbulence).
      float amplitude = 0.05 + uPressure * 0.17 + uTurbulence * 0.16;
      vec3 displaced = position + normal * noise * amplitude;

      vNoise = noise;
      vec4 worldPos = modelMatrix * vec4(displaced, 1.0);
      vWorldPosition = worldPos.xyz;
      gl_Position = projectionMatrix * viewMatrix * worldPos;
    }
  `;

  // ── Fragment shader ─────────────────────────────────────────────────────────
  // Composes: fire/ice domain coloring + hot embers + cool veins + Fresnel aura.
  const fragmentShader = /* glsl */ `
    varying vec3 vPosition;
    varying vec3 vNormal;
    varying vec3 vWorldPosition;
    varying float vNoise;

    uniform float uTime;
    uniform float uPressure;
    uniform float uActivity;
    uniform float uHealth;
    uniform float uExcite;
    uniform vec3 uExciteColor;
    uniform vec3 uPhaseColor;
    uniform vec3 uCameraPosition;
    uniform float uPulseRate;
    uniform float uGlow;
    uniform float uKrypto;

    // Same noise as vertex shader (DRY for portability)
    vec3 mod289(vec3 x) { return x - floor(x * (1.0 / 289.0)) * 289.0; }
    vec4 mod289(vec4 x) { return x - floor(x * (1.0 / 289.0)) * 289.0; }
    vec4 permute(vec4 x) { return mod289(((x*34.0)+1.0)*x); }
    vec4 taylorInvSqrt(vec4 r) { return 1.79284291400159 - 0.85373472095314 * r; }
    float snoise(vec3 v) {
      const vec2 C = vec2(1.0/6.0, 1.0/3.0);
      const vec4 D = vec4(0.0, 0.5, 1.0, 2.0);
      vec3 i  = floor(v + dot(v, C.yyy));
      vec3 x0 = v - i + dot(i, C.xxx);
      vec3 g = step(x0.yzx, x0.xyz);
      vec3 l = 1.0 - g;
      vec3 i1 = min(g.xyz, l.zxy);
      vec3 i2 = max(g.xyz, l.zxy);
      vec3 x1 = x0 - i1 + C.xxx;
      vec3 x2 = x0 - i2 + C.yyy;
      vec3 x3 = x0 - D.yyy;
      i = mod289(i);
      vec4 p = permute(permute(permute(
                 i.z + vec4(0.0, i1.z, i2.z, 1.0))
               + i.y + vec4(0.0, i1.y, i2.y, 1.0))
               + i.x + vec4(0.0, i1.x, i2.x, 1.0));
      float n_ = 0.142857142857;
      vec3 ns = n_ * D.wyz - D.xzx;
      vec4 j = p - 49.0 * floor(p * ns.z * ns.z);
      vec4 x_ = floor(j * ns.z);
      vec4 y_ = floor(j - 7.0 * x_);
      vec4 x = x_ *ns.x + ns.yyyy;
      vec4 y = y_ *ns.x + ns.yyyy;
      vec4 h = 1.0 - abs(x) - abs(y);
      vec4 b0 = vec4(x.xy, y.xy);
      vec4 b1 = vec4(x.zw, y.zw);
      vec4 s0 = floor(b0)*2.0 + 1.0;
      vec4 s1 = floor(b1)*2.0 + 1.0;
      vec4 sh = -step(h, vec4(0.0));
      vec4 a0 = b0.xzyw + s0.xzyw*sh.xxyy;
      vec4 a1 = b1.xzyw + s1.xzyw*sh.zzww;
      vec3 p0 = vec3(a0.xy, h.x);
      vec3 p1 = vec3(a0.zw, h.y);
      vec3 p2 = vec3(a1.xy, h.z);
      vec3 p3 = vec3(a1.zw, h.w);
      vec4 norm = taylorInvSqrt(vec4(dot(p0,p0), dot(p1,p1), dot(p2,p2), dot(p3,p3)));
      p0 *= norm.x; p1 *= norm.y; p2 *= norm.z; p3 *= norm.w;
      vec4 m = max(0.6 - vec4(dot(x0,x0), dot(x1,x1), dot(x2,x2), dot(x3,x3)), 0.0);
      m = m * m;
      return 42.0 * dot(m*m, vec4(dot(p0,x0), dot(p1,x1), dot(p2,x2), dot(p3,x3)));
    }

    void main() {
      // Tonal spread derived from the live phase color: a deep molten core,
      // the phase mid-tone, and a near-white hot tip. Every phase gets its
      // own living hue (cyan planning, ember implementation, gold testing…)
      // while keeping the internal fire/ice contrast that gives the orb depth.
      vec3 CORE  = uPhaseColor * 0.26;                 // shadowed core
      vec3 MID   = uPhaseColor;                        // the phase itself
      vec3 HOT   = mix(uPhaseColor, vec3(1.0), 0.58);  // incandescent tip
      vec3 ICE   = MID;
      vec3 ICE2  = CORE;
      vec3 EMBER = mix(CORE, MID, 0.6);
      vec3 FLAME = HOT;

      // High-frequency texture noise (independent of vNoise from vertex)
      float t = uTime * 0.4;
      float n1 = snoise(vPosition * 3.5 + vec3(t * 0.7, 0.0, 0.0));
      float n2 = snoise(vPosition * 9.0 + vec3(0.0, t * 1.1, 0.0));
      float n3 = snoise(vPosition * 18.0 + vec3(t * 0.3, 0.0, t * 0.5));
      float texture_ = n1 * 0.55 + n2 * 0.30 + n3 * 0.15;

      // Fire/ice domain — boundary swirls around the sphere driven by Y axis,
      // time, and the noise field. smoothstep softens the transition.
      float boundary = sin(vPosition.y * 2.5 + uTime * 0.4 + texture_ * 1.8);
      float fireMix = smoothstep(-0.35, 0.35, boundary);

      // Base color: blend fire and ice
      vec3 fireBase = mix(EMBER, FLAME, smoothstep(0.0, 1.0, n2 + 0.5));
      vec3 iceBase = mix(ICE2, ICE, smoothstep(0.0, 1.0, n2 + 0.5));
      vec3 color = mix(iceBase, fireBase, fireMix);

      // Hot embers — high-frequency noise peaks glow brighter
      float embers = smoothstep(0.45, 0.85, n2);
      color += FLAME * embers * (0.4 + uActivity * 0.6);

      // Cool veins — low-frequency troughs gleam ice-blue
      float veins = smoothstep(-0.85, -0.45, n3);
      color += ICE * veins * 0.35;

      // Fresnel rim — only bright at grazing angles (steep curve). State glow
      // intensity scales the rim halo (idle dim → success bloom bright).
      vec3 viewDir = normalize(uCameraPosition - vWorldPosition);
      float fresnel = 1.0 - max(0.0, dot(viewDir, normalize(vNormal)));
      fresnel = pow(fresnel, 3.2);  // Steep curve: only edges glow
      color += HOT * fresnel * (0.25 + uGlow * 0.7);  // phase-tinted rim halo

      // Activity + state pulse — global brightness modulation, frequency driven
      // by the state's pulseRate so "slows down" reads as a calmer breath.
      float pulse = 1.0 + sin(uTime * (1.0 + uActivity * 2.0) * uPulseRate) * (0.05 + 0.05 * uActivity);
      color *= pulse;

      // Kryptonite — a bright jade halo that pulses then settles (success).
      vec3 JADE = vec3(0.0, 1.0, 0.62);
      float kpulse = uKrypto * (0.6 + 0.4 * sin(uTime * 6.0));
      color += JADE * fresnel * kpulse * 1.8;
      color += JADE * kpulse * 0.25;

      // Excitement — the orb runs hot on a stimulus. Blend the surface
      // toward the reaction color (ember on request, jade on success,
      // rose on failure) with the noise field driving molten streaks,
      // plus a bright rim flare in a lightened tint.
      vec3 flareTint = mix(uExciteColor, vec3(1.0), 0.3);
      float emberMix = uExcite * (0.45 + 0.25 * smoothstep(0.1, 0.8, n1 + 0.5));
      color = mix(color, uExciteColor * (1.2 + n2 * 0.4), emberMix);
      color += flareTint * fresnel * uExcite * 1.6;
      // Heat-up brightness: excited orbs burn brighter overall.
      color *= 1.0 + uExcite * 0.45;

      // Health → overall warmth multiplier (low health = dimmer)
      color *= mix(0.55, 1.0, uHealth);

      // Soft tone-map to keep highlights from blowing out
      color = color / (1.0 + color * 0.5);

      gl_FragColor = vec4(color, 1.0);
    }
  `;

  // ── Convert hex string → THREE.Vector3 ──────────────────────────────────────
  function hexToVec3(hex: string): THREE.Vector3 {
    const c = new THREE.Color(hex);
    return new THREE.Vector3(c.r, c.g, c.b);
  }

  // ── Setup scene ─────────────────────────────────────────────────────────────
  function setup() {
    if (!container) return;

    scene = new THREE.Scene();

    const aspect = 1;
    camera = new THREE.PerspectiveCamera(45, aspect, 0.1, 100);
    camera.position.set(0, 0, 3.2);

    renderer = new THREE.WebGLRenderer({
      antialias: true,
      alpha: true,
      powerPreference: 'high-performance'
    });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    renderer.setSize(size, size);
    renderer.setClearColor(0x000000, 0); // transparent — body bg shows through
    container.appendChild(renderer.domElement);

    const geometry = new THREE.IcosahedronGeometry(1.0, 64);

    material = new THREE.ShaderMaterial({
      vertexShader,
      fragmentShader,
      uniforms: {
        uTime: { value: 0 },
        uPressure: { value: pressure },
        uActivity: { value: activity },
        uHealth: { value: health },
        uExcite: { value: 0 },
        uExciteColor: { value: new THREE.Vector3(1.0, 0.45, 0.05) },
        uPhaseColor: { value: hexToVec3(hue) },
        uCameraPosition: { value: camera.position.clone() },
        // Living-orb motion uniforms.
        uPulseRate: { value: pulseRate },
        uGlow: { value: glowIntensity },
        uTurbulence: { value: turbulence },
        uKrypto: { value: 0 }
      },
      transparent: false
    });

    mesh = new THREE.Mesh(geometry, material);
    scene.add(mesh);

    // Minimal ambient — let the shader define the lighting
    const ambient = new THREE.AmbientLight(0x0088dd, 0.02);
    scene.add(ambient);

    lastTime = performance.now();
    animate();
  }

  function animate() {
    const now = performance.now();
    const dt = (now - lastTime) / 1000;
    lastTime = now;

    if (material) {
      material.uniforms.uTime.value += dt;
      // Gentle interpolation of dynamic inputs for smoothness
      material.uniforms.uPressure.value += (pressure - material.uniforms.uPressure.value) * 0.05;
      material.uniforms.uActivity.value += (activity - material.uniforms.uActivity.value) * 0.05;
      material.uniforms.uHealth.value += (health - material.uniforms.uHealth.value) * 0.05;

      // Smoothly chase the living-orb motion params so state changes animate
      // (no hard cut), unless reduce-motion is set (then snap).
      const k = reduceMotion ? 1 : 0.06;
      material.uniforms.uPulseRate.value += (pulseRate - material.uniforms.uPulseRate.value) * k;
      material.uniforms.uGlow.value += (glowIntensity - material.uniforms.uGlow.value) * k;
      material.uniforms.uTurbulence.value += (turbulence - material.uniforms.uTurbulence.value) * k;

      // Kryptonite pulses 2–3× then settles: decay the envelope toward a low
      // steady glow over ~1.4s once triggered.
      if (special === 'kryptonite') {
        krypto = Math.max(0.18, krypto - dt / 1.4);
      } else {
        krypto = Math.max(0, krypto - dt * 2);
      }
      material.uniforms.uKrypto.value += (krypto - material.uniforms.uKrypto.value) * Math.min(1, dt * 8);

      // Excitement envelope — decays back to calm over ~2.5s. The shader
      // sees a softened copy so the orange glow ramps in/out smoothly.
      excite = Math.max(0, excite - (dt * 1000) / EXCITE_DURATION_MS);
      const eased = smoothstep01(excite);
      material.uniforms.uExcite.value +=
        (eased - material.uniforms.uExcite.value) * Math.min(1, dt * 10);

      // Rotation: the state's spinSpeed sets the base rate; excitement spins it
      // up further. `hardStop` freezes it; `reverseSpin` runs it backwards;
      // `stutter` jitters the rate intermittently. reduce-motion calms it all.
      const excSpin = spinMultiplier(excite, 9);
      let base = special === 'hardStop' ? 0 : spinSpeed;
      if (special === 'reverseSpin') base = -Math.abs(spinSpeed);
      if (special === 'stutter') base *= 0.6 + Math.random() * 0.9;
      const rm = reduceMotion ? 0.35 : 1;
      const spin = base * excSpin * rm;
      mesh.rotation.y += 0.0038 * spin;
      mesh.rotation.x += 0.0015 * spin;

      // Shake: front-loaded jitter (excite^3) — a sharp rattle on impact
      // that settles before the glow fades. Success blooms without one.
      const shake = shakes(stimulusKind) ? shakeAmplitude(excite, 0.07) : 0;
      if (shake > 0.0005) {
        mesh.position.set(
          (Math.random() - 0.5) * shake,
          (Math.random() - 0.5) * shake,
          (Math.random() - 0.5) * shake
        );
      } else if (mesh.position.lengthSq() > 0) {
        mesh.position.set(0, 0, 0);
      }
    }

    if (renderer && scene && camera) {
      renderer.render(scene, camera);
    }
    frameId = requestAnimationFrame(animate);
  }

  // ── Reactive uniform updates ────────────────────────────────────────────────
  $: if (material && hue) {
    material.uniforms.uPhaseColor.value = hexToVec3(hue);
  }
  $: if (renderer && size) {
    renderer.setSize(size, size);
  }

  onMount(setup);

  onDestroy(() => {
    if (frameId !== null) cancelAnimationFrame(frameId);
    if (renderer) {
      renderer.dispose();
      if (renderer.domElement.parentNode) {
        renderer.domElement.parentNode.removeChild(renderer.domElement);
      }
    }
    if (material) material.dispose();
    if (mesh) {
      mesh.geometry.dispose();
    }
  });
</script>

<div
  bind:this={container}
  class="forge-container relative inline-block"
  style="width: {size}px; height: {size}px;"
  aria-label="The Forge — live agent cognition visualization"
></div>
