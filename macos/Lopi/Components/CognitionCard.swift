import SwiftUI

/// A live agent tile. The whole card breathes with the agent's generation
/// `activity`, the ring tracks context `pressure`, and tokens/sec + cost roll
/// in real time. Score and verifier verdict surface as they arrive.
struct CognitionCard: View {
    let agent: LiveAgent
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var isThinking: Bool { agent.active && PhaseStyle.isActive(agent.phase) }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            header
            Text(agent.goal)
                .font(Konjo.sans(13))
                .foregroundStyle(agent.active ? Konjo.fg : Konjo.fgDim)
                .lineLimit(2)
                .frame(maxWidth: .infinity, alignment: .leading)
            Spacer(minLength: 0)
            metrics
            footer
        }
        .padding(16)
        .frame(height: 210)
        .background(cardBackground)
        .overlay(
            RoundedRectangle(cornerRadius: 14)
                .stroke(agent.accent.opacity(agent.active ? 0.45 : 0.12), lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: 14))
        .konjoGlow(agent.accent, radius: 10, active: isThinking)
        .opacity(agent.active ? 1 : 0.7)
    }

    // MARK: Sections

    private var header: some View {
        HStack(spacing: 8) {
            PulseOrb(color: agent.accent, active: isThinking)
            VStack(alignment: .leading, spacing: 1) {
                Text(agent.phase.capitalized)
                    .font(Konjo.mono(11, weight: .semibold))
                    .foregroundStyle(agent.accent)
                Text(agent.id.prefix(8))
                    .font(Konjo.mono(9))
                    .foregroundStyle(Konjo.fgMute)
            }
            Spacer()
            VStack(alignment: .trailing, spacing: 1) {
                RollingNumber(value: agent.tokensPerSec, format: "%.0f",
                              font: Konjo.mono(15, weight: .semibold),
                              color: isThinking ? Konjo.fg : Konjo.fgDim)
                Text("tok/s")
                    .font(Konjo.mono(8))
                    .foregroundStyle(Konjo.fgMute)
            }
        }
    }

    private var metrics: some View {
        HStack(spacing: 14) {
            PressureRing(value: agent.pressure, label: "PRESS", color: agent.accent)
            VStack(alignment: .leading, spacing: 7) {
                metricRow(label: "activity", value: agent.activity, color: agent.accent)
                if let pass = agent.testPassRate {
                    metricRow(label: "tests", value: pass,
                              color: pass >= 0.8 ? Konjo.ok : Konjo.warn)
                }
            }
        }
    }

    private func metricRow(label: String, value: Double, color: Color) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack {
                Text(label.uppercased())
                    .font(Konjo.mono(8)).foregroundStyle(Konjo.fgMute)
                Spacer()
                Text("\(Int(value * 100))%")
                    .font(Konjo.mono(9)).foregroundStyle(Konjo.fgDim)
            }
            Meter(value: value, color: color)
        }
    }

    private var footer: some View {
        HStack(spacing: 8) {
            if let branch = agent.branch { Pill(text: branch, color: Konjo.fgDim) }
            if agent.attempt > 1 { Pill(text: "try \(agent.attempt)", color: Konjo.warn) }
            if let passed = agent.verdictPassed {
                Pill(text: passed ? "verified" : "rejected", color: passed ? Konjo.ok : Konjo.err)
            }
            Spacer()
            Text(String(format: "$%.4f", agent.costUsd))
                .font(Konjo.mono(10))
                .foregroundStyle(Konjo.fgDim)
        }
    }

    private var cardBackground: some View {
        ZStack {
            Konjo.bg1
            // Accent wash that intensifies with live activity.
            LinearGradient(
                colors: [agent.accent.opacity(isThinking ? 0.16 : 0.04), .clear],
                startPoint: .topLeading, endPoint: .bottomTrailing
            )
        }
    }
}

/// A status orb that emits a soft expanding pulse ring while the agent is
/// actively computing. Honors reduce-motion by holding a static glow.
struct PulseOrb: View {
    let color: Color
    var active: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        ZStack {
            if active && !reduceMotion {
                TimelineView(.animation) { timeline in
                    let t = timeline.date.timeIntervalSinceReferenceDate
                    let phase = (sin(t * 2.2) + 1) / 2 // 0...1
                    Circle()
                        .stroke(color.opacity(0.6 * (1 - phase)), lineWidth: 2)
                        .scaleEffect(1 + phase * 1.4)
                        .frame(width: 12, height: 12)
                }
            }
            Circle()
                .fill(color)
                .frame(width: 11, height: 11)
                .konjoGlow(color, radius: 5, active: active)
        }
        .frame(width: 28, height: 28)
    }
}
