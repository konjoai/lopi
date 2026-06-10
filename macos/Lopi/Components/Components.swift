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
                .shadow(color: color.opacity(0.8), radius: 4)
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

    private var color: Color { Konjo.statusColor(status) }

    private var active: Bool {
        !["success", "done", "failed", "queued"].contains(status.lowercased())
    }

    var body: some View {
        Circle()
            .fill(color)
            .frame(width: 10, height: 10)
            .scaleEffect(active && pulse ? 1.25 : 1.0)
            .opacity(active && pulse ? 0.7 : 1.0)
            .animation(active ? .easeInOut(duration: 1.0).repeatForever(autoreverses: true) : .default, value: pulse)
            .onAppear { pulse = active }
            .onChange(of: active) { newValue in pulse = newValue }
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
