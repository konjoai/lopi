import SwiftUI
import UniformTypeIdentifiers
import LopiStacksKit

// Shared stack UI primitives — the native analogues of the web `Toggle`,
// `Combo`, segmented rows, the iteration pill, and the cardbar icon button.
// Reused by `StackCardView`, the popovers, and `StackControlDockView` so a card
// and the dock read identically, exactly as the web components share them.

extension UTType {
    /// Drag payload identifying a committed card being reordered within its pane.
    static var lopiCardReorder: UTType { UTType(exportedAs: "com.konjo.lopi.card-reorder") }
    /// Drag payload identifying a stack pane being reordered within the grid.
    static var lopiStackReorder: UTType { UTType(exportedAs: "com.konjo.lopi.stack-reorder") }
}

/// Drag-and-drop payload for reordering committed cards within one pane
/// (`StackStore.reorderInPaneRelative`). The draft card (index `-1`) is never
/// draggable, so `index` here is always a valid `pane.cards` position.
struct CardDragPayload: Codable, Transferable {
    var paneKey: String
    var index: Int

    static var transferRepresentation: some TransferRepresentation {
        CodableRepresentation(contentType: .lopiCardReorder)
    }
}

/// Drag-and-drop payload for reordering whole stack panes in the grid
/// (`StackStore.reorderStacksInPanes`).
struct StackDragPayload: Codable, Transferable {
    var index: Int

    static var transferRepresentation: some TransferRepresentation {
        CodableRepresentation(contentType: .lopiStackReorder)
    }
}

/// A pill toggle (web `Toggle.svelte`) — a track with a sliding knob, accent-lit
/// when on. Mouse-driven, which is right for Mac.
struct StackToggle: View {
    var isOn: Bool
    var accent: Color = Konjo.ice
    var onToggle: () -> Void

    var body: some View {
        Button(action: onToggle) {
            ZStack(alignment: isOn ? .trailing : .leading) {
                Capsule()
                    .fill(isOn ? accent.opacity(0.28) : Color.white.opacity(0.06))
                    .overlay(Capsule().stroke(isOn ? accent.opacity(0.6) : Konjo.line, lineWidth: 1))
                Circle()
                    .fill(isOn ? accent : Konjo.fgMute)
                    .padding(2)
            }
            .frame(width: 32, height: 18)
            .animation(.easeOut(duration: 0.16), value: isOn)
        }
        .buttonStyle(.plain)
    }
}

/// A horizontal segmented control (on-fail / budget / frequency / AM-PM rows).
struct StackSegmented<T: Hashable>: View {
    var options: [(T, String)]
    var selected: T
    var accent: Color = Konjo.sun
    var onSelect: (T) -> Void

    var body: some View {
        HStack(spacing: 0) {
            ForEach(Array(options.enumerated()), id: \.offset) { idx, opt in
                Button { onSelect(opt.0) } label: {
                    Text(opt.1)
                        .font(Konjo.mono(10))
                        .padding(.horizontal, 10).padding(.vertical, 4)
                        .foregroundStyle(opt.0 == selected ? accent : Konjo.fgDim)
                        .background(opt.0 == selected ? accent.opacity(0.16) : Color.clear)
                }
                .buttonStyle(.plain)
                if idx < options.count - 1 {
                    Rectangle().fill(Konjo.line).frame(width: 1, height: 18)
                }
            }
        }
        .overlay(RoundedRectangle(cornerRadius: 6).stroke(Konjo.line, lineWidth: 1))
        .clipShape(RoundedRectangle(cornerRadius: 6))
    }
}

/// A ±1 number stepper (web `Combo`, simplified for cron hour/minute). Clamps to
/// `[min, max]` and calls back with the new value.
struct StackCombo: View {
    var value: Int
    var range: ClosedRange<Int>
    var onChange: (Int) -> Void

    var body: some View {
        HStack(spacing: 0) {
            step("−", -1)
            Text(String(value))
                .font(Konjo.mono(11)).foregroundStyle(Konjo.fg)
                .frame(width: 30)
            step("+", 1)
        }
        .overlay(RoundedRectangle(cornerRadius: 6).stroke(Konjo.line, lineWidth: 1))
        .clipShape(RoundedRectangle(cornerRadius: 6))
    }

    private func step(_ glyph: String, _ delta: Int) -> some View {
        Button {
            onChange(min(range.upperBound, max(range.lowerBound, value + delta)))
        } label: {
            Text(glyph).font(Konjo.mono(13)).foregroundStyle(Konjo.ice).frame(width: 22, height: 24)
        }
        .buttonStyle(.plain)
    }
}

/// The warm iteration pill — the ×N ceiling with hover-revealed steppers. Shared
/// by the cardbar (per-loop) and the dock (chain loop-count). The steppers'
/// reveal is an animated width expansion (0 → 64pt, matching the web pill's
/// `max-width` transition) rather than a hard cut.
struct IterationPill: View {
    var value: Int
    /// Card scope floors at `0` = "off"; the dock (stack loop-count) treats `1`
    /// as "off" and keeps `0` as the `∞` sentinel. Drives both the label and
    /// whether `off`/`∞` render without a `×`.
    var offAtZero: Bool = false
    /// Set once this loop is both actively running (`card.status ==
    /// .running` / the stack's own `RunPhase == .running`) AND has a real
    /// repeat configured — matches web's `loopRunning`/`stackLoopRunning`.
    /// Non-nil swaps the static `loop` glyph for a spinning arc, the `×N`
    /// label for this live "current/total" text, and adds the slow glow —
    /// mirroring `StackCard.svelte`/`StackControlDock.svelte`'s `.running`.
    var runningLabel: String? = nil
    var onStep: (Int) -> Void
    @State private var hovering = false
    @State private var pulse = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var isOff: Bool { offAtZero ? value == 0 : value == 1 }
    private var isRunning: Bool { runningLabel != nil }

    private var displayText: String {
        if let runningLabel { return runningLabel }
        if offAtZero { return value == 0 ? "off" : "×\(value)" }
        let label = maxIterationsLabel(value)
        return value <= 1 ? label : "×\(label)"
    }

    private var tint: Color { isOff ? Konjo.fgDim : FacetAccent.iteration }

    var body: some View {
        HStack(spacing: 0) {
            HStack(spacing: 5) {
                if isRunning {
                    SpinnerArc().frame(width: 11, height: 11)
                } else {
                    Image(systemName: "arrow.triangle.2.circlepath").font(.system(size: 11, weight: .bold))
                }
                Text(displayText).font(Konjo.mono(11, weight: .bold))
            }
            .padding(.horizontal, 9).frame(height: 29)
            HStack(spacing: 0) {
                stepper("−", -1)
                stepper("+", 1)
            }
            .frame(width: hovering ? 64 : 0, height: 29, alignment: .leading)
            .clipped()
        }
        .foregroundStyle(tint)
        .background(isOff ? Color.white.opacity(0.05) : Konjo.ember.opacity(0.09))
        .overlay(
            RoundedRectangle(cornerRadius: 6)
                .stroke(tint.opacity(isRunning && pulse ? 0.95 : (isOff ? 0.4 : 0.5)), lineWidth: 1)
        )
        .shadow(color: isRunning ? Konjo.ember.opacity(pulse ? 0.45 : 0) : .clear, radius: pulse ? 8 : 0)
        .clipShape(RoundedRectangle(cornerRadius: 6))
        .onHover { isHovering in
            withAnimation(.timingCurve(0.5, 0, 0.2, 1, duration: 0.24)) {
                hovering = isHovering
            }
        }
        .onAppear { startPulseIfRunning() }
        .onChange(of: isRunning) { _, _ in startPulseIfRunning() }
        .help(isOff ? "off · runs once, no repeat" : (value == 0 && !offAtZero ? "unlimited · runs until guardrails or goal stop it" : ""))
    }

    /// Kicks off (or stops) the glow's repeating animation. Driven from
    /// `onAppear`/`onChange` rather than a bare `.animation(value:)` on
    /// `pulse` — `repeatForever` needs an explicit `withAnimation` to start,
    /// and toggling `isRunning` off must also snap `pulse` back to its rest
    /// state rather than freezing mid-pulse.
    private func startPulseIfRunning() {
        guard isRunning, !reduceMotion else {
            pulse = false
            return
        }
        withAnimation(.easeInOut(duration: 1.2).repeatForever(autoreverses: true)) {
            pulse = true
        }
    }

    private func stepper(_ glyph: String, _ delta: Int) -> some View {
        Button { onStep(delta) } label: {
            Text(glyph).font(Konjo.mono(14)).frame(width: 26, height: 29)
                .overlay(Rectangle().fill(tint.opacity(0.35)).frame(width: 1), alignment: .leading)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(tint)
    }
}

/// A continuously-rotating three-quarter arc — the native analogue of web's
/// `ICONS.spinner` (a partial-circle SVG animated via CSS `spin`). Used only
/// by `IterationPill` while a loop is actively running, so the pill itself
/// (not just its glow) reads as "in motion".
private struct SpinnerArc: View {
    @State private var rotation = 0.0
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        Circle()
            .trim(from: 0, to: 0.75)
            .stroke(style: StrokeStyle(lineWidth: 2, lineCap: .round))
            .rotationEffect(.degrees(rotation))
            .onAppear {
                guard !reduceMotion else { return }
                withAnimation(.linear(duration: 1.1).repeatForever(autoreverses: false)) {
                    rotation = 360
                }
            }
    }
}

/// A cardbar / dock icon button, accent-lit when its facet is active. Optional
/// count badge (evals), text label (the draft's `+ add`), danger tint (delete),
/// and disabled state (the `+ add` before the draft is hot).
struct CardbarButton: View {
    var systemImage: String
    var active = false
    var accent: Color = Konjo.ice
    var count: Int? = nil
    var label: String? = nil
    var danger = false
    var disabled = false
    var help: String = ""
    var action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 5) {
                Image(systemName: systemImage).font(.system(size: 12))
                if let count { Text("\(count)").font(Konjo.mono(9, weight: .bold)) }
                if let label { Text(label).font(Konjo.mono(11, weight: .bold)) }
            }
            // Padding INSIDE the minimum frame (web's CSS `min-width` is
            // border-box, so its padding is absorbed within 29px, not added
            // on top) — padding-then-frame here keeps an icon-only button at
            // ~29pt instead of inflating to ~43pt.
            .padding(.horizontal, label == nil ? 7 : 11)
            .frame(minWidth: 29, minHeight: 29)
            .foregroundStyle(active ? accent : Konjo.fgMute)
            .background(active ? accent.opacity(0.09) : Color.clear)
            .overlay(RoundedRectangle(cornerRadius: 6).stroke(active ? accent.opacity(0.5) : Konjo.line, lineWidth: 1))
            .clipShape(RoundedRectangle(cornerRadius: 6))
            // A `Color.clear` background does not hit-test on macOS — without
            // this, only the opaque icon/text glyphs are clickable, not the
            // rest of the button's visual bounds.
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(disabled)
        .opacity(disabled ? 0.4 : 1)
        .help(help)
        .accessibilityIdentifier(help)
    }
}

/// ProvenanceChips — a card's origin chips (Creation-Flow-1 §4). Not a card view
/// (the "one card view" rule is about not forking StackCardView) — just the
/// shared chip cluster the draft card and every committed card both render, so
/// the two never drift. Color semantics match the templates menu's sections:
///   • prompt template → a SUN chip with the name, *replacing* the teal alias chip
///   • stack template  → a VIOLET chip with the name, PLUS the card's teal alias chip
///   • no template     → the teal alias chip
/// Every symbol size is constrained explicitly — an unconstrained glyph blows the
/// chip apart (the mockup bug §4 calls out).
struct ProvenanceChips: View {
    var alias: String?
    var tpl: String?
    var tplKind: TplKind?
    /// The card's resolved repo path (`card.config.repo`), if set — rendered
    /// as its own chip so an inline `@org/repo` pick stays visible after
    /// commit instead of vanishing once the goal text's `@token` is stripped.
    /// Callers pass the already-resolved *label* (via `repoLabelForPath`), not
    /// the raw path, so this view stays free of the repo catalog.
    var repoLabel: String?

    private var isPrompt: Bool { tplKind == .prompt && tpl != nil }
    private var isStack: Bool { tplKind == .stack && tpl != nil }
    // The teal alias chip shows for a stack-template loop and a no-template card,
    // but never for a prompt template (its sun chip *is* the identity).
    private var showAlias: Bool { alias != nil && !isPrompt }

    var body: some View {
        HStack(spacing: 9) {
            if isPrompt, let tpl { chip(text: tpl, icon: "doc", color: Konjo.sun) }
            if isStack, let tpl { chip(text: tpl, icon: "square.3.layers.3d", color: Konjo.stackViolet) }
            if showAlias, let alias { chip(text: ":\(alias)", icon: "wrench", color: Konjo.stackTeal) }
            if let repoLabel { chip(text: repoLabel, icon: "folder", color: Konjo.stackSky) }
        }
    }

    private func chip(text: String, icon: String, color: Color) -> some View {
        HStack(spacing: 5) {
            Image(systemName: icon).font(.system(size: 11))
            Text(text).font(Konjo.mono(12.5))
        }
        .foregroundStyle(color)
        .padding(.horizontal, 10).padding(.vertical, 3)
        .background(color.opacity(0.08))
        .overlay(RoundedRectangle(cornerRadius: 7).stroke(color.opacity(0.4), lineWidth: 1))
        .clipShape(RoundedRectangle(cornerRadius: 7))
    }
}

/// A hide-inactive summary row (icon label + text), matching the card/dock
/// `.sumln` rows.
struct SummaryRow: View {
    var systemImage: String
    var label: String
    var accent: Color
    var text: String

    var body: some View {
        HStack(spacing: 7) {
            HStack(spacing: 4) {
                Image(systemName: systemImage).font(.system(size: 9))
                Text(label.uppercased()).font(Konjo.mono(8)).tracking(0.6)
            }
            .foregroundStyle(accent).frame(width: 66, alignment: .leading)
            Text(text).font(Konjo.mono(9.5)).foregroundStyle(Konjo.fgDim).lineLimit(1)
            Spacer(minLength: 0)
        }
    }
}
