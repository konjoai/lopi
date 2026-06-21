#include <metal_stdlib>
using namespace metal;

// ── Forge orb — a faithful Metal port of the web UI's GLSL sphere shader
// (web/src/lib/forge/Forge.svelte). Renders an ever-rotating, noise-morphing
// sphere of fire and ice whose entire palette is driven by the live phase
// color. Runs per-pixel via SwiftUI's `.colorEffect`, so the silhouette
// actually deforms (not a fixed circle) exactly like the WebGL version.

// Ashima 3D simplex noise — identical math to the web shader.
static float3 mod289(float3 x) { return x - floor(x * (1.0 / 289.0)) * 289.0; }
static float4 mod289(float4 x) { return x - floor(x * (1.0 / 289.0)) * 289.0; }
static float4 permute(float4 x) { return mod289(((x * 34.0) + 1.0) * x); }
static float4 taylorInvSqrt(float4 r) { return 1.79284291400159 - 0.85373472095314 * r; }

static float snoise(float3 v) {
    const float2 C = float2(1.0 / 6.0, 1.0 / 3.0);
    const float4 D = float4(0.0, 0.5, 1.0, 2.0);
    float3 i  = floor(v + dot(v, C.yyy));
    float3 x0 = v - i + dot(i, C.xxx);
    float3 g = step(x0.yzx, x0.xyz);
    float3 l = 1.0 - g;
    float3 i1 = min(g.xyz, l.zxy);
    float3 i2 = max(g.xyz, l.zxy);
    float3 x1 = x0 - i1 + C.xxx;
    float3 x2 = x0 - i2 + C.yyy;
    float3 x3 = x0 - D.yyy;
    i = mod289(i);
    float4 p = permute(permute(permute(
                 i.z + float4(0.0, i1.z, i2.z, 1.0))
               + i.y + float4(0.0, i1.y, i2.y, 1.0))
               + i.x + float4(0.0, i1.x, i2.x, 1.0));
    float n_ = 0.142857142857;
    float3 ns = n_ * D.wyz - D.xzx;
    float4 j = p - 49.0 * floor(p * ns.z * ns.z);
    float4 x_ = floor(j * ns.z);
    float4 y_ = floor(j - 7.0 * x_);
    float4 x = x_ * ns.x + ns.yyyy;
    float4 y = y_ * ns.x + ns.yyyy;
    float4 h = 1.0 - abs(x) - abs(y);
    float4 b0 = float4(x.xy, y.xy);
    float4 b1 = float4(x.zw, y.zw);
    float4 s0 = floor(b0) * 2.0 + 1.0;
    float4 s1 = floor(b1) * 2.0 + 1.0;
    float4 sh = -step(h, float4(0.0));
    float4 a0 = b0.xzyw + s0.xzyw * sh.xxyy;
    float4 a1 = b1.xzyw + s1.xzyw * sh.zzww;
    float3 p0 = float3(a0.xy, h.x);
    float3 p1 = float3(a0.zw, h.y);
    float3 p2 = float3(a1.xy, h.z);
    float3 p3 = float3(a1.zw, h.w);
    float4 norm = taylorInvSqrt(float4(dot(p0, p0), dot(p1, p1), dot(p2, p2), dot(p3, p3)));
    p0 *= norm.x; p1 *= norm.y; p2 *= norm.z; p3 *= norm.w;
    float4 m = max(0.6 - float4(dot(x0, x0), dot(x1, x1), dot(x2, x2), dot(x3, x3)), 0.0);
    m = m * m;
    return 42.0 * dot(m * m, float4(dot(p0, x0), dot(p1, x1), dot(p2, x2), dot(p3, x3)));
}

// Rotate a direction about the Y then X axes (the orb's calm drift / spin).
static float3 rotate3(float3 v, float ry, float rx) {
    float cy = cos(ry), sy = sin(ry);
    v = float3(cy * v.x + sy * v.z, v.y, -sy * v.x + cy * v.z);
    float cx = cos(rx), sx = sin(rx);
    v = float3(v.x, cx * v.y - sx * v.z, sx * v.y + cx * v.z);
    return v;
}

[[ stitchable ]]
half4 forgeOrb(float2 pos, half4 color, float4 bounds,
               float time, float pressure, float activity,
               float health, half4 phaseColor,
               float spin, float excite, half4 exciteColor) {
    // Centered, normalized coordinates; y points up like the GLSL version.
    float half_ = min(bounds.z, bounds.w) * 0.5;
    float2 uv = (pos - bounds.zw * 0.5) / half_;
    uv.y = -uv.y;
    float rr = length(uv);

    // Rotation angle is supplied by the host (base drift + stimulus spin-up).
    float ry = spin;
    float rx = spin * 0.4;

    // ── Silhouette displacement ──────────────────────────────────────────
    // Sample the noise field along the view-plane direction so the *outline*
    // wobbles and morphs, matching the web's vertex displacement. Pressure
    // scales the amplitude (0.05 calm → 0.22 turbulent).
    float3 dir = rr > 0.0001 ? float3(uv.x, uv.y, 0.0) / rr : float3(0.0, 0.0, 1.0);
    float3 rs = rotate3(dir, ry, rx);
    float td = time * (0.18 + activity * 0.32);
    float dn1 = snoise(rs * 1.8 + float3(td, 0.0, 0.0));
    float dn2 = snoise(rs * 4.5 + float3(0.0, td * 1.3, 0.0));
    // Fluid, ever-changing silhouette. The earlier "violence" was the shake +
    // fast spin + surface churn (all still damped); this is the shape morph the
    // operator actually wants — kept rich at ~55% of the original amplitude.
    float disp = (dn1 * 0.7 + dn2 * 0.3) * (0.05 + pressure * 0.17) * 0.55;

    float baseR = 0.82;
    float radius = baseR + disp;
    if (rr > radius) {
        return half4(0.0);  // outside the morphing silhouette
    }

    // Sphere normal at this pixel, then the rotated surface sample point.
    float z = sqrt(max(0.0, radius * radius - rr * rr));
    float3 n = normalize(float3(uv.x, uv.y, z));
    float3 sp = rotate3(n, ry, rx);

    // ── Fragment coloring (port of the web fragment shader) ───────────────
    float3 phase = float3(phaseColor.rgb);
    float3 CORE  = phase * 0.26;
    float3 MID   = phase;
    float3 HOT   = mix(phase, float3(1.0), 0.58);
    float3 ICE   = MID;
    float3 ICE2  = CORE;
    float3 EMBER = mix(CORE, MID, 0.6);
    float3 FLAME = HOT;

    float t = time * 0.2;
    float fn1 = snoise(sp * 3.5 + float3(t * 0.7, 0.0, 0.0));
    float fn2 = snoise(sp * 9.0 + float3(0.0, t * 1.1, 0.0));
    float fn3 = snoise(sp * 18.0 + float3(t * 0.3, 0.0, t * 0.5));
    float texture_ = fn1 * 0.55 + fn2 * 0.30 + fn3 * 0.15;

    float boundary = sin(sp.y * 2.5 + time * 0.2 + texture_ * 1.8);
    float fireMix = smoothstep(-0.35, 0.35, boundary);
    float3 fireBase = mix(EMBER, FLAME, smoothstep(0.0, 1.0, fn2 + 0.5));
    float3 iceBase  = mix(ICE2, ICE, smoothstep(0.0, 1.0, fn2 + 0.5));
    float3 col = mix(iceBase, fireBase, fireMix);

    float embers = smoothstep(0.45, 0.85, fn2);
    col += FLAME * embers * (0.4 + activity * 0.6);
    float veins = smoothstep(-0.85, -0.45, fn3);
    col += ICE * veins * 0.35;

    // Fresnel rim — view direction is +z in screen space.
    float fres = pow(1.0 - max(0.0, n.z), 3.2);
    col += HOT * fres * 0.5;

    float pulse = 1.0 + sin(time * (1.0 + activity * 1.5)) * 0.03 * activity;
    col *= pulse;

    // Excitement — the orb runs hot on a stimulus: the surface blends toward
    // the reaction color (ember on request, jade on success, rose on failure)
    // with the noise driving molten streaks, plus a bright rim flare. 1:1 with
    // the web fragment shader.
    float3 exc = float3(exciteColor.rgb);
    float3 flareTint = mix(exc, float3(1.0), 0.3);
    float emberMix = excite * (0.45 + 0.25 * smoothstep(0.1, 0.8, fn1 + 0.5));
    col = mix(col, exc * (1.2 + fn2 * 0.4), emberMix);
    col += flareTint * fres * excite * 1.6;
    col *= 1.0 + excite * 0.45;

    col *= mix(0.55, 1.0, health);
    col = col / (1.0 + col * 0.5);  // soft tone-map

    // Antialias the morphing rim and premultiply the resulting alpha.
    float edge = smoothstep(radius, radius - 0.015, rr);
    return half4(half3(col) * half(edge), half(edge));
}
