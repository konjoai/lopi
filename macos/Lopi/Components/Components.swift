import SwiftUI

/// Small connection LED + label, matching the web UI's top-bar indicator.
struct ConnectionLED: View {
    let state: ConnectionState

    private var color: Color {
        switch state {
        case .live: return Konjo.ok
        case .connecting: return Konjo.warn
        case .offline: return Konjo.err
        }
    }

    private var label: String {
        switch state {
        case .live: return "live"
        case .connecting: return "connecting"
        case .offline: return "offline"
        }
    }

    var body: some View {
        HStack(spacing: 6) {
            Circle()
                .fill(color)
                .frame(width: 8, height: 8)
            Text(label)
                .font(Konjo.mono(11))
                .foregroundStyle(Konjo.fgDim)
        }
    }
}

/// A status orb whose color encodes a task phase. Pulses while active.
struct StatusOrb: View {
    let status: String
    @State private var pulse = false

    // Single source of truth for phase → color + liveness (shared with orbs).
    private var color: Color { PhaseStyle.color(status) }
    private var active: Bool { PhaseStyle.isActive(status) }

    var body: some View {
        Circle()
            .fill(color)
            .frame(width: 10, height: 10)
            .scaleEffect(active && pulse ? 1.25 : 1.0)
            .opacity(active && pulse ? 0.7 : 1.0)
            .animation(active ? .easeInOut(duration: 1.0).repeatForever(autoreverses: true) : .default, value: pulse)
            .onAppear { pulse = active }
            .onChange(of: active) { _, newValue in pulse = newValue }
    }
}

/// One metric tile in the dashboard stats strip.
struct StatCard: View {
    let label: String
    let value: String
    var accent: Color = Konjo.fg

    var body: some View {
        KonjoPanel {
            VStack(alignment: .leading, spacing: 6) {
                Text(label.uppercased())
                    .font(Konjo.mono(10))
                    .foregroundStyle(Konjo.fgMute)
                Text(value)
                    .font(Konjo.sans(26, weight: .semibold))
                    .foregroundStyle(accent)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

/// A radial gauge (0...1) — used for the budget readout.
struct RadialGauge: View {
    /// Fraction in 0...1.
    let fraction: Double
    let caption: String

    private var color: Color {
        switch fraction {
        case ..<0.6: return Konjo.ok
        case ..<0.85: return Konjo.warn
        default: return Konjo.err
        }
    }

    var body: some View {
        ZStack {
            Circle()
                .stroke(Konjo.line2, lineWidth: 10)
            Circle()
                .trim(from: 0, to: min(max(fraction, 0), 1))
                .stroke(color, style: StrokeStyle(lineWidth: 10, lineCap: .round))
                .rotationEffect(.degrees(-90))
                .animation(.easeOut(duration: 0.5), value: fraction)
            VStack(spacing: 2) {
                Text("\(Int(fraction * 100))%")
                    .font(Konjo.sans(22, weight: .semibold))
                    .foregroundStyle(color)
                Text(caption)
                    .font(Konjo.mono(10))
                    .foregroundStyle(Konjo.fgMute)
            }
        }
        .frame(width: 140, height: 140)
    }
}

/// Unified Konjo button — a hairline-bordered pill that tints to its accent on
/// hover and dips on press (the tactile "press" feel the web UI uses). Drop-in
/// via `.buttonStyle(KonjoButtonStyle())` or the `.konjoButton()` helper.
struct KonjoButtonStyle: ButtonStyle {
    var accent: Color = Konjo.ice
    var prominent: Bool = false
    @State private var hovering = false

    func makeBody(configuration: Configuration) -> some View {
        let pressed = configuration.isPressed
        return configuration.label
            .font(Konjo.mono(11, weight: .medium))
            .foregroundStyle(prominent ? Konjo.bg : (hovering ? accent : Konjo.fgDim))
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
            .background(
                RoundedRectangle(cornerRadius: 7)
                    .fill(prominent ? accent.opacity(hovering ? 1 : 0.9)
                                    : accent.opacity(hovering ? 0.14 : 0.04))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 7)
                    .stroke(prominent ? .clear : accent.opacity(hovering ? 0.5 : 0.18), lineWidth: 1)
            )
            .scaleEffect(pressed ? 0.96 : 1.0)
            .shadow(color: prominent && hovering ? accent.opacity(0.45) : .clear, radius: 10)
            .animation(.easeOut(duration: 0.14), value: pressed)
            .animation(.easeOut(duration: 0.18), value: hovering)
            .onHover { hovering = $0 }
            .contentShape(Rectangle())
    }
}

extension View {
    /// Apply the standard Konjo button styling.
    func konjoButton(_ accent: Color = Konjo.ice, prominent: Bool = false) -> some View {
        buttonStyle(KonjoButtonStyle(accent: accent, prominent: prominent))
    }
}

/// Dark, hairline-bordered field chrome to replace the stock `.roundedBorder`
/// (which renders a jarring light inset on the near-black void). Works for both
/// `TextField` and `TextEditor`; the border lifts to the accent while focused.
struct KonjoFieldModifier: ViewModifier {
    var focused: Bool = false
    var accent: Color = Konjo.ice

    func body(content: Content) -> some View {
        content
            .textFieldStyle(.plain)
            .font(Konjo.mono(12))
            .foregroundStyle(Konjo.fg)
            .tint(accent)
            .scrollContentBackground(.hidden)
            .padding(.horizontal, 10)
            .padding(.vertical, 7)
            .background(Konjo.deep.opacity(0.55))
            .overlay(
                RoundedRectangle(cornerRadius: 7)
                    .stroke(focused ? accent.opacity(0.6) : Konjo.line2, lineWidth: 1)
            )
            .clipShape(RoundedRectangle(cornerRadius: 7))
            .animation(.easeOut(duration: 0.16), value: focused)
    }
}

extension View {
    /// Apply Konjo dark-field chrome. Pass a `@FocusState`-driven `focused`
    /// flag to get the accent focus ring; omit it for static styling.
    func konjoField(focused: Bool = false, accent: Color = Konjo.ice) -> some View {
        modifier(KonjoFieldModifier(focused: focused, accent: accent))
    }
}

/// A non-fatal error/info banner that the user can dismiss.
struct BannerView: View {
    let text: String
    let onDismiss: () -> Void

    var body: some View {
        HStack {
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(Konjo.warn)
            Text(text)
                .font(Konjo.sans(12))
                .foregroundStyle(Konjo.fg)
            Spacer()
            Button(action: onDismiss) {
                Image(systemName: "xmark")
            }
            .buttonStyle(.plain)
            .foregroundStyle(Konjo.fgDim)
        }
        .padding(10)
        .background(Konjo.bg2)
        .overlay(RoundedRectangle(cornerRadius: 8).stroke(Konjo.warn.opacity(0.5), lineWidth: 1))
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }
}
