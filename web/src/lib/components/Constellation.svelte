<!--
  Constellation — full-canvas 3D scene where every running agent orbits a
  central beacon. The view of the whole portfolio at a glance.

  Per-agent body:
    distance from center → priority (close=critical, far=low)
    body size           → context pressure
    rotation speed      → activity
    aura color          → current phase
    trail length        → recent activity over time

  Interaction:
    hover  → tooltip with goal + phase + repo
    click  → makes that agent active and navigates back to /

  Performance: a single shared Three.js scene with one renderer. Each agent
  body uses a streamlined shader (Fresnel + lightweight noise — no fire/ice
  composition) so 20+ bodies render cheaply.
-->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import * as THREE from 'three';
  import {
    agents,
    activeAgentId,
    selectAgent,
    PHASE_COLORS,
    type AgentState
  } from '$lib/stores/agents';
  import type { Priority } from '$lib/types';
  import {
    computeConnections,
    connectionsFor,
    type Connection
  } from '$lib/forge/connections';
  import {
    exciteLevel,
    smoothstep01,
    shakeAmplitude,
    spinMultiplier
  } from '$lib/forge/excitement';

  // ── Tooltip state (rendered outside the canvas as DOM) ────────────────────
  export let onSelect: (id: string) => void = () => {};

  let container: HTMLDivElement;
  let canvas: HTMLCanvasElement;

  let scene: THREE.Scene;
  let camera: THREE.PerspectiveCamera;
  let renderer: THREE.WebGLRenderer;
  let raycaster: THREE.Raycaster;
  let pointer: THREE.Vector2;
  let clock: THREE.Clock;
  let frameId: number | null = null;

  // Background starfield + center beacon
  let starField: THREE.Points;
  let beacon: THREE.Mesh;

  // Per-agent runtime state
  interface Body {
    id: string;
    mesh: THREE.Mesh;
    material: THREE.ShaderMaterial;
    orbitRadius: number;
    orbitSpeed: number;
    orbitPhase: number;
    orbitTilt: number;
    trail: THREE.Line;
    trailPositions: Float32Array;
    trailLen: number;
    /** Last stimulus timestamp (ms) — drives the ember flash + spin-up. */
    stimulus: number;
  }
  const bodies = new Map<string, Body>();

  // Per-connection runtime state — one Three.js Line per connected pair
  interface ConnectionRuntime {
    spec: Connection;
    line: THREE.Line;
    geometry: THREE.BufferGeometry;
    positions: Float32Array; // length 6 (2 vertices × xyz)
  }
  const connections = new Map<string, ConnectionRuntime>();

  // Latest computed connection list — reactive on agents map
  let connectionList: Connection[] = [];

  // Hover state
  let hoveredId: string | null = null;
  let hoverScreen = { x: 0, y: 0 };

  // ── Vertex shader (light: only Fresnel-friendly varyings) ─────────────────
  const vertexShader = /* glsl */ `
    varying vec3 vNormal;
    varying vec3 vWorldPos;

    uniform float uTime;
    uniform float uPressure;

    // Cheap 3D hash noise — much faster than simplex when we don't need
    // deep texture detail per fragment.
    float hash(vec3 p) {
      p = fract(p * 0.3183099 + vec3(0.71, 0.113, 0.419));
      p *= 17.0;
      return fract(p.x * p.y * p.z * (p.x + p.y + p.z));
    }

    void main() {
      vNormal = normalize(normalMatrix * normal);

      // Gentle radial pulse from noise — pressure modulates amplitude
      float n = hash(position * 4.0 + vec3(uTime * 0.4, 0.0, 0.0));
      float disp = (n - 0.5) * (0.04 + uPressure * 0.10);
      vec3 displaced = position + normal * disp;

      vec4 wp = modelMatrix * vec4(displaced, 1.0);
      vWorldPos = wp.xyz;
      gl_Position = projectionMatrix * viewMatrix * wp;
    }
  `;

  // ── Fragment shader (Fresnel aura + soft inner color) ────────────────────
  const fragmentShader = /* glsl */ `
    varying vec3 vNormal;
    varying vec3 vWorldPos;

    uniform float uTime;
    uniform float uActivity;
    uniform vec3 uPhaseColor;
    uniform vec3 uCameraPosition;
    uniform float uActive;       // 0 (dim) or 1 (focused)
    uniform float uExcite;       // 0..1 — incoming-request flash

    void main() {
      vec3 viewDir = normalize(uCameraPosition - vWorldPos);
      float fresnel = 1.0 - max(0.0, dot(viewDir, normalize(vNormal)));
      fresnel = pow(fresnel, 2.0);

      // Inner soft glow
      vec3 inner = uPhaseColor * 0.45;
      // Outer aura (Fresnel-driven)
      vec3 aura = uPhaseColor * fresnel * (1.6 + uActivity * 1.2);

      // Pulse with activity
      float pulse = 1.0 + sin(uTime * (1.0 + uActivity * 2.5)) * 0.08 * uActivity;
      vec3 color = (inner + aura) * pulse;

      // Active body brighter
      color *= mix(0.85, 1.6, uActive);

      // Excitement — incoming request flashes the body ember orange.
      vec3 EXCITE_ORANGE = vec3(1.0, 0.45, 0.05);
      color = mix(color, EXCITE_ORANGE * (1.2 + fresnel * 1.4), uExcite * 0.7);
      color *= 1.0 + uExcite * 0.5;

      // Soft tone-map
      color = color / (1.0 + color * 0.5);
      gl_FragColor = vec4(color, 1.0);
    }
  `;

  // ── Color helper ──────────────────────────────────────────────────────────
  function hexToVec3(hex: string): THREE.Vector3 {
    const c = new THREE.Color(hex);
    return new THREE.Vector3(c.r, c.g, c.b);
  }

  // ── Hash a string id → stable [0, 1) ──────────────────────────────────────
  function strHash01(s: string): number {
    let h = 2166136261 >>> 0;
    for (let i = 0; i < s.length; i++) {
      h ^= s.charCodeAt(i);
      h = Math.imul(h, 16777619);
    }
    return (h >>> 0) / 0xffffffff;
  }

  // ── Priority → orbit radius (close=critical, far=low) ─────────────────────
  function priorityRadius(p: Priority | string): number {
    switch (p) {
      case 'Critical':
        return 1.6;
      case 'High':
        return 2.4;
      case 'Normal':
        return 3.4;
      case 'Low':
        return 4.4;
      default:
        return 3.4;
    }
  }

  // ── Build body for a new agent ────────────────────────────────────────────
  function addBody(agent: AgentState) {
    const phaseColor = PHASE_COLORS[agent.phase] ?? PHASE_COLORS.Boot;

    // Geometry: small icosahedron, 16-subdivisions plenty for the size.
    const geometry = new THREE.IcosahedronGeometry(0.22, 16);

    const material = new THREE.ShaderMaterial({
      vertexShader,
      fragmentShader,
      uniforms: {
        uTime: { value: 0 },
        uPressure: { value: agent.pressure },
        uActivity: { value: agent.activity },
        uPhaseColor: { value: hexToVec3(phaseColor) },
        uCameraPosition: { value: camera.position.clone() },
        uActive: { value: agent.id === $activeAgentId ? 1.0 : 0.0 },
        uExcite: { value: 0 }
      }
    });

    const mesh = new THREE.Mesh(geometry, material);
    mesh.userData.agentId = agent.id;
    scene.add(mesh);

    // Stable orbit per agent — derived from id hash so a refresh keeps the
    // visual layout the same.
    const h1 = strHash01(agent.id);
    const h2 = strHash01(agent.id + ':2');
    const h3 = strHash01(agent.id + ':3');
    const orbitRadius = priorityRadius('Normal'); // refined below from taskStatus
    const orbitSpeed = 0.18 + h1 * 0.18;
    const orbitPhase = h2 * Math.PI * 2;
    const orbitTilt = (h3 - 0.5) * 0.6;

    // Trail — last 64 positions as a line
    const TRAIL_SEGMENTS = 64;
    const trailPositions = new Float32Array(TRAIL_SEGMENTS * 3);
    const trailGeo = new THREE.BufferGeometry();
    trailGeo.setAttribute('position', new THREE.BufferAttribute(trailPositions, 3));
    const trailMat = new THREE.LineBasicMaterial({
      color: phaseColor,
      transparent: true,
      opacity: 0.35
    });
    const trail = new THREE.Line(trailGeo, trailMat);
    scene.add(trail);

    bodies.set(agent.id, {
      id: agent.id,
      mesh,
      material,
      orbitRadius,
      orbitSpeed,
      orbitPhase,
      orbitTilt,
      trail,
      trailPositions,
      trailLen: 0,
      stimulus: agent.stimulus
    });
  }

  // ── Update body uniforms + orbit radius from current state ────────────────
  function updateBody(body: Body, agent: AgentState) {
    body.material.uniforms.uPressure.value = agent.pressure;
    body.material.uniforms.uActivity.value = agent.activity;
    body.stimulus = agent.stimulus;
    const phaseColor = PHASE_COLORS[agent.phase] ?? PHASE_COLORS.Boot;
    body.material.uniforms.uPhaseColor.value = hexToVec3(phaseColor);
    body.material.uniforms.uActive.value = agent.id === $activeAgentId ? 1.0 : 0.0;

    // Trail color follows phase color
    (body.trail.material as THREE.LineBasicMaterial).color = new THREE.Color(phaseColor);

    // Body scale ∝ pressure (visual: full agent feels bigger)
    const scale = 0.7 + agent.pressure * 0.8;
    body.mesh.scale.setScalar(scale);
  }

  // ── Remove body ───────────────────────────────────────────────────────────
  function removeBody(id: string) {
    const b = bodies.get(id);
    if (!b) return;
    scene.remove(b.mesh);
    scene.remove(b.trail);
    b.mesh.geometry.dispose();
    b.material.dispose();
    b.trail.geometry.dispose();
    (b.trail.material as THREE.LineBasicMaterial).dispose();
    bodies.delete(id);
  }

  // ── Connection management ────────────────────────────────────────────────
  // Add a new line for a connection between two existing bodies.
  function addConnection(spec: Connection) {
    const positions = new Float32Array(6);
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3));

    // Color: use the dominant phase color of the two endpoints when they
    // sync, otherwise fall back to a neutral mist that doesn't compete with
    // the agent body auras.
    const a = bodies.get(spec.fromId);
    const b = bodies.get(spec.toId);
    const phaseColor = spec.phaseSync && a && b
      ? PHASE_COLORS[($agents.get(spec.fromId)?.phase ?? 'Boot') as keyof typeof PHASE_COLORS]
      : '#7a8a9a';

    const material = new THREE.LineBasicMaterial({
      color: new THREE.Color(phaseColor),
      transparent: true,
      // Strength → opacity in [0.10, 0.45] — always subtle, never noisy
      opacity: 0.10 + spec.strength * 0.35
    });

    const line = new THREE.Line(geometry, material);
    line.frustumCulled = false; // endpoints are always within view; skip per-frame cull math
    scene.add(line);
    connections.set(spec.id, { spec, line, geometry, positions });
  }

  function updateConnectionMaterial(rt: ConnectionRuntime, spec: Connection) {
    rt.spec = spec;
    const a = bodies.get(spec.fromId);
    const b = bodies.get(spec.toId);
    const phaseColor = spec.phaseSync && a && b
      ? PHASE_COLORS[($agents.get(spec.fromId)?.phase ?? 'Boot') as keyof typeof PHASE_COLORS]
      : '#7a8a9a';
    const mat = rt.line.material as THREE.LineBasicMaterial;
    mat.color.set(phaseColor);
    mat.opacity = 0.10 + spec.strength * 0.35;
  }

  function removeConnection(id: string) {
    const rt = connections.get(id);
    if (!rt) return;
    scene.remove(rt.line);
    rt.geometry.dispose();
    (rt.line.material as THREE.LineBasicMaterial).dispose();
    connections.delete(id);
  }

  function syncConnections(specs: Connection[]) {
    if (!scene) return;
    const next = new Map(specs.map((s) => [s.id, s]));

    // Add or update
    for (const [id, spec] of next) {
      const rt = connections.get(id);
      if (!rt) addConnection(spec);
      else updateConnectionMaterial(rt, spec);
    }

    // Remove stale
    for (const id of [...connections.keys()]) {
      if (!next.has(id)) removeConnection(id);
    }
  }

  // ── Sync the body map with the agent store ───────────────────────────────
  function syncBodies(map: Map<string, AgentState>) {
    if (!scene) return;
    // Add or update
    for (const [id, agent] of map) {
      let b = bodies.get(id);
      if (!b) {
        addBody(agent);
        b = bodies.get(id)!;
      }
      updateBody(b, agent);
    }
    // Remove gone agents
    for (const id of [...bodies.keys()]) {
      if (!map.has(id)) removeBody(id);
    }
    // Recompute connections after the body set has stabilized
    connectionList = computeConnections(map);
    syncConnections(connectionList);
  }

  // Re-sync whenever the agent store changes
  $: if (scene && $agents) syncBodies($agents);
  $: if (scene && $activeAgentId !== undefined) {
    for (const b of bodies.values()) {
      b.material.uniforms.uActive.value = b.id === $activeAgentId ? 1.0 : 0.0;
    }
  }

  // ── Starfield (background depth) ──────────────────────────────────────────
  function buildStarfield() {
    const N = 800;
    const positions = new Float32Array(N * 3);
    for (let i = 0; i < N; i++) {
      // Distribute on a sphere shell at radius 50–80
      const r = 50 + Math.random() * 30;
      const theta = Math.random() * Math.PI * 2;
      const phi = Math.acos(2 * Math.random() - 1);
      positions[i * 3] = r * Math.sin(phi) * Math.cos(theta);
      positions[i * 3 + 1] = r * Math.sin(phi) * Math.sin(theta);
      positions[i * 3 + 2] = r * Math.cos(phi);
    }
    const geo = new THREE.BufferGeometry();
    geo.setAttribute('position', new THREE.BufferAttribute(positions, 3));
    const mat = new THREE.PointsMaterial({
      color: 0xffffff,
      size: 0.08,
      transparent: true,
      opacity: 0.55,
      sizeAttenuation: true
    });
    starField = new THREE.Points(geo, mat);
    scene.add(starField);
  }

  // ── Center beacon — the lopi core ────────────────────────────────────────
  function buildBeacon() {
    const geo = new THREE.IcosahedronGeometry(0.18, 32);
    const mat = new THREE.ShaderMaterial({
      transparent: true,
      uniforms: {
        uTime: { value: 0 },
        uCameraPosition: { value: camera.position.clone() }
      },
      vertexShader: /* glsl */ `
        varying vec3 vN; varying vec3 vP;
        void main(){
          vN = normalize(normalMatrix * normal);
          vec4 wp = modelMatrix * vec4(position, 1.0);
          vP = wp.xyz;
          gl_Position = projectionMatrix * viewMatrix * wp;
        }
      `,
      fragmentShader: /* glsl */ `
        varying vec3 vN; varying vec3 vP;
        uniform float uTime;
        uniform vec3 uCameraPosition;
        void main(){
          vec3 v = normalize(uCameraPosition - vP);
          float f = 1.0 - max(0.0, dot(v, normalize(vN)));
          f = pow(f, 1.6);
          float pulse = 0.85 + sin(uTime * 0.7) * 0.15;
          // Mix ice and ember at the heart of the constellation
          vec3 col = mix(vec3(0.0,0.83,1.0), vec3(1.0,0.27,0.0), 0.5 + 0.5*sin(uTime*0.4));
          vec3 c = col * (0.55 + f * 1.4) * pulse;
          c = c / (1.0 + c * 0.4);
          gl_FragColor = vec4(c, 1.0);
        }
      `
    });
    beacon = new THREE.Mesh(geo, mat);
    scene.add(beacon);
  }

  // ── Click + hover handling via raycaster ─────────────────────────────────
  function updatePointer(e: MouseEvent) {
    const rect = canvas.getBoundingClientRect();
    pointer.x = ((e.clientX - rect.left) / rect.width) * 2 - 1;
    pointer.y = -((e.clientY - rect.top) / rect.height) * 2 + 1;
    hoverScreen = { x: e.clientX - rect.left, y: e.clientY - rect.top };
  }

  function onMouseMove(e: MouseEvent) {
    updatePointer(e);
    raycaster.setFromCamera(pointer, camera);
    const meshes = [...bodies.values()].map((b) => b.mesh);
    const hits = raycaster.intersectObjects(meshes, false);
    hoveredId = hits.length > 0 ? (hits[0].object.userData.agentId as string) : null;
    canvas.style.cursor = hoveredId ? 'pointer' : 'default';
  }

  function onMouseClick(e: MouseEvent) {
    updatePointer(e);
    raycaster.setFromCamera(pointer, camera);
    const meshes = [...bodies.values()].map((b) => b.mesh);
    const hits = raycaster.intersectObjects(meshes, false);
    if (hits.length > 0) {
      const id = hits[0].object.userData.agentId as string;
      selectAgent(id);
      onSelect(id);
    }
  }

  // ── Animate loop ─────────────────────────────────────────────────────────
  function animate() {
    const t = clock.getElapsedTime();

    // Update beacon
    if (beacon) {
      const m = beacon.material as THREE.ShaderMaterial;
      m.uniforms.uTime.value = t;
      m.uniforms.uCameraPosition.value = camera.position.clone();
      beacon.rotation.y += 0.003;
      beacon.rotation.x += 0.001;
    }

    // Slow background star spin for parallax
    if (starField) starField.rotation.y += 0.0003;

    // Update each body
    for (const body of bodies.values()) {
      const x = Math.cos(body.orbitPhase + t * body.orbitSpeed) * body.orbitRadius;
      const z = Math.sin(body.orbitPhase + t * body.orbitSpeed) * body.orbitRadius;
      const y = Math.sin(body.orbitTilt + t * body.orbitSpeed * 0.4) * 0.6;
      body.mesh.position.set(x, y, z);
      body.material.uniforms.uTime.value = t;
      body.material.uniforms.uCameraPosition.value = camera.position.clone();

      // Excitement envelope from the stimulus timestamp: 1 at impact,
      // decaying to 0 over 2.5s. Drives the ember flash, a quick shake,
      // and a spin-up that settles back to the ambient drift.
      const excite = exciteLevel(body.stimulus, Date.now());
      body.material.uniforms.uExcite.value = smoothstep01(excite);

      // Self-rotation — up to ~8× faster while excited
      body.mesh.rotation.y += 0.01 * spinMultiplier(excite, 7);

      // Shake: front-loaded positional rattle on impact
      const shake = shakeAmplitude(excite, 0.08);
      if (shake > 0.001) {
        body.mesh.position.x += (Math.random() - 0.5) * shake;
        body.mesh.position.y += (Math.random() - 0.5) * shake;
        body.mesh.position.z += (Math.random() - 0.5) * shake;
      }

      // Trail update — shift positions left, append current
      const positions = body.trailPositions;
      for (let i = 0; i < positions.length - 3; i++) {
        positions[i] = positions[i + 3];
      }
      const last = positions.length - 3;
      positions[last] = x;
      positions[last + 1] = y;
      positions[last + 2] = z;
      body.trail.geometry.attributes.position.needsUpdate = true;
      body.trailLen = Math.min(body.trailLen + 1, positions.length / 3);
      body.trail.geometry.setDrawRange(
        Math.max(0, positions.length / 3 - body.trailLen),
        body.trailLen
      );
    }

    // Update connection lines — tracks moving endpoints + pulses on phase sync
    for (const rt of connections.values()) {
      const a = bodies.get(rt.spec.fromId);
      const b = bodies.get(rt.spec.toId);
      if (!a || !b) continue;
      rt.positions[0] = a.mesh.position.x;
      rt.positions[1] = a.mesh.position.y;
      rt.positions[2] = a.mesh.position.z;
      rt.positions[3] = b.mesh.position.x;
      rt.positions[4] = b.mesh.position.y;
      rt.positions[5] = b.mesh.position.z;
      rt.geometry.attributes.position.needsUpdate = true;

      // Phase-sync pulse: shared-phase connections breathe with time.
      // Non-sync connections hold steady opacity (subtle ambient association).
      const mat = rt.line.material as THREE.LineBasicMaterial;
      const baseOpacity = 0.10 + rt.spec.strength * 0.35;
      if (rt.spec.phaseSync) {
        mat.opacity = baseOpacity + Math.sin(t * 2.0) * 0.10;
      } else {
        mat.opacity = baseOpacity;
      }
    }

    // Slow camera drift around the system for cinematic feel
    const camRadius = 8.5;
    camera.position.x = Math.cos(t * 0.05) * camRadius;
    camera.position.z = Math.sin(t * 0.05) * camRadius;
    camera.position.y = 2.5 + Math.sin(t * 0.07) * 0.4;
    camera.lookAt(0, 0, 0);

    renderer.render(scene, camera);
    frameId = requestAnimationFrame(animate);
  }

  // ── Resize handler ───────────────────────────────────────────────────────
  function onResize() {
    if (!container || !renderer || !camera) return;
    const w = container.clientWidth;
    const h = container.clientHeight;
    renderer.setSize(w, h);
    camera.aspect = w / h;
    camera.updateProjectionMatrix();
  }

  // ── Lifecycle ────────────────────────────────────────────────────────────
  onMount(() => {
    scene = new THREE.Scene();
    scene.background = new THREE.Color(0x050505);

    const w = container.clientWidth;
    const h = container.clientHeight;
    camera = new THREE.PerspectiveCamera(50, w / h, 0.1, 200);
    camera.position.set(8.5, 2.5, 0);
    camera.lookAt(0, 0, 0);

    renderer = new THREE.WebGLRenderer({
      canvas,
      antialias: true,
      powerPreference: 'high-performance'
    });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    renderer.setSize(w, h);

    raycaster = new THREE.Raycaster();
    pointer = new THREE.Vector2();
    clock = new THREE.Clock();

    buildStarfield();
    buildBeacon();

    // Initial sync from store
    syncBodies($agents);

    animate();
    window.addEventListener('resize', onResize);
  });

  onDestroy(() => {
    if (frameId !== null) cancelAnimationFrame(frameId);
    window.removeEventListener('resize', onResize);
    for (const id of [...connections.keys()]) removeConnection(id);
    for (const id of [...bodies.keys()]) removeBody(id);
    if (beacon) {
      scene.remove(beacon);
      beacon.geometry.dispose();
      (beacon.material as THREE.ShaderMaterial).dispose();
    }
    if (starField) {
      scene.remove(starField);
      starField.geometry.dispose();
      (starField.material as THREE.PointsMaterial).dispose();
    }
    if (renderer) renderer.dispose();
  });

  // ── Hovered agent for tooltip ────────────────────────────────────────────
  $: hoveredAgent = hoveredId ? $agents.get(hoveredId) ?? null : null;
  $: hoveredConnections = hoveredId ? connectionsFor(connectionList, hoveredId) : [];

  // Resolve the peer id for a connection from the perspective of the hovered agent
  function peerId(c: Connection, self: string): string {
    return c.fromId === self ? c.toId : c.fromId;
  }
</script>

<div bind:this={container} class="constellation-container relative w-full h-[calc(100vh-3rem)]">
  <canvas
    bind:this={canvas}
    on:mousemove={onMouseMove}
    on:click={onMouseClick}
    on:keydown={(e) => {
      if (e.key === 'Escape') onSelect('');
    }}
    class="block w-full h-full focus:outline-none"
    tabindex="0"
  ></canvas>

  {#if hoveredAgent}
    <div
      class="absolute pointer-events-none px-4 py-3 rounded-lg bg-konjo-deep/95 border border-white/10 backdrop-blur-md text-sm shadow-2xl max-w-sm"
      style:left="{hoverScreen.x + 16}px"
      style:top="{hoverScreen.y + 16}px"
      style:border-color={PHASE_COLORS[hoveredAgent.phase]}
    >
      <div class="font-display font-medium leading-tight mb-1">{hoveredAgent.goal}</div>
      <div class="font-mono text-[10px] uppercase tracking-widest opacity-50">
        {hoveredAgent.repo}
        {#if hoveredAgent.repo}·{/if}
        <span style:color={PHASE_COLORS[hoveredAgent.phase]}>{hoveredAgent.phase}</span>
        ·
        <span class="tabular-nums">{Math.round(hoveredAgent.pressure * 100)}%</span>
      </div>

      {#if hoveredConnections.length > 0}
        <div class="mt-3 pt-3 border-t border-white/5">
          <div class="font-mono text-[9px] uppercase tracking-widest opacity-40 mb-1.5">
            connected to {hoveredConnections.length} agent{hoveredConnections.length === 1 ? '' : 's'}
          </div>
          <div class="space-y-1">
            {#each hoveredConnections.slice(0, 3) as c (c.id)}
              {@const peer = $agents.get(peerId(c, hoveredAgent.id))}
              {#if peer}
                <div class="flex items-center gap-2 text-[11px]">
                  <span
                    class="w-1.5 h-1.5 rounded-full flex-shrink-0"
                    class:animate-pulse={c.phaseSync}
                    style:background={c.phaseSync ? PHASE_COLORS[peer.phase] : '#7a8a9a'}
                  ></span>
                  <span class="opacity-80 truncate flex-1">{peer.goal}</span>
                </div>
                <div class="font-mono text-[9px] opacity-35 ml-3.5 -mt-0.5">
                  {c.reasons.join(' · ')}
                </div>
              {/if}
            {/each}
            {#if hoveredConnections.length > 3}
              <div class="font-mono text-[9px] opacity-30 ml-3.5">
                + {hoveredConnections.length - 3} more
              </div>
            {/if}
          </div>
        </div>
      {/if}

      <div class="mt-2 font-mono text-[9px] opacity-30">click to focus</div>
    </div>
  {/if}

  <!-- Empty-state overlay -->
  {#if $agents.size === 0}
    <div
      class="absolute inset-0 flex items-center justify-center pointer-events-none"
    >
      <div class="text-center opacity-60">
        <div class="font-display text-3xl mb-2">no agents in flight</div>
        <div class="font-mono text-xs uppercase tracking-widest opacity-50">
          waiting for the constellation to populate
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .constellation-container {
    background: radial-gradient(circle at center, #0a0a0a 0%, #000000 100%);
  }
</style>
