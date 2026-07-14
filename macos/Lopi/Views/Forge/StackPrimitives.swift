import SwiftUI

// Shared stack UI primitives — the native analogues of the web `Toggle`,
// `Combo`, segmented rows, the iteration pill, and the cardbar icon button.
// Reused by `StackCardView`, the popovers, and `StackControlDockView` so a card
// and the dock read identically, exactly as the web components share them.

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
/// by the cardbar (per-loop) and the dock (chain loop-count).
struct IterationPill: View {
    var value: Int
    /// Card scope floors at `0` = "off"; the dock (stack loop-count) keeps the
    /// `∞` sentinel. Drives both the label and whether `off` reads without a `×`.
    var offAtZero: Bool = false
    var onStep: (Int) -> Void
    @State private var hovering = false

    private var displayText: String {
        if offAtZero { return value == 0 ? "off" : "×\(value)" }
        return "×\(maxIterationsLabel(value))"
    }

    var body: some View {
        HStack(spacing: 0) {
            HStack(spacing: 5) {
                Image(systemName: "arrow.triangle.2.circlepath").font(.system(size: 11, weight: .bold))
                Text(displayText).font(Konjo.mono(11, weight: .bold))
            }
            .padding(.horizontal, 9).frame(height: 29)
            if hovering {
                stepper("−", -1)
                stepper("+", 1)
            }
        }
        .foregroundStyle(FacetAccent.iteration)
        .background(Konjo.ember.opacity(0.09))
        .overlay(RoundedRectangle(cornerRadius: 6).stroke(FacetAccent.iteration.opacity(0.5), lineWidth: 1))
        .clipShape(RoundedRectangle(cornerRadius: 6))
        .onHover { hovering = $0 }
    }

    private func stepper(_ glyph: String, _ delta: Int) -> some View {
        Button { onStep(delta) } label: {
            Text(glyph).font(Konjo.mono(14)).frame(width: 26, height: 29)
                .overlay(Rectangle().fill(FacetAccent.iteration.opacity(0.35)).frame(width: 1), alignment: .leading)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(FacetAccent.iteration)
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
