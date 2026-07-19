import SwiftUI
import LopiStacksKit

// Presentation-layer helpers for the iOS Stack Loops screens — derives the
// four-bucket Overview grouping and the per-card display status from real
// `StackCard`/`LiveAgent` state (no fabricated fields on the model itself).
// Mirrors the intent of `CardOrb`/`OrbStateMap` (macOS) but only needs the
// coarser four-state vocabulary the design calls for, not the full orb motion
// spec — see `docs/ops/IOS_RESEARCH_1_SPIKE.md` for why `CardOrbState.swift`
// itself stayed app-target-only rather than being reused directly here.

/// The four lifecycle buckets the Overview screen groups stacks into.
enum OverviewPhase: String, CaseIterable, Identifiable {
    case queued, running, testing, done

    var id: String { rawValue }
    var label: String { rawValue.uppercased() }

    var color: Color {
        switch self {
        case .queued: return Konjo.fgMute
        case .running: return Konjo.ice
        case .testing: return Konjo.violet
        case .done: return Konjo.jade
        }
    }
}

/// Per-card display status. Not stored on `StackCard` — computed fresh from
/// `CardStatus` plus (once the card has launched) the live agent's phase and
/// verdict, so it can never drift from the real state.
enum LoopCardDisplayStatus: Equatable {
    case queued
    case running
    case testing
    case blocked(reason: String)
    case done

    var label: String {
        switch self {
        case .queued: return "QUEUED"
        case .running: return "RUNNING"
        case .testing: return "TESTING"
        case .blocked: return "BLOCKED"
        case .done: return "DONE"
        }
    }

    var color: Color {
        switch self {
        case .queued: return Konjo.fgMute
        case .running: return Konjo.ice
        case .testing: return Konjo.violet
        case .blocked: return Konjo.rose
        case .done: return Konjo.jade
        }
    }

    var isActive: Bool {
        switch self {
        case .running, .testing: return true
        default: return false
        }
    }
}

enum StackDisplay {
    /// Which of the four Overview buckets a pane currently belongs in.
    static func overviewPhase(for pane: StackPaneState, liveAgents: [String: LiveAgent]) -> OverviewPhase {
        guard !pane.cards.isEmpty else { return .queued }
        if pane.cards.allSatisfy({ $0.status == .done }) { return .done }
        if pane.cards.contains(where: { cardStatus($0, liveAgents: liveAgents) == .testing }) {
            return .testing
        }
        if pane.cards.contains(where: { cardStatus($0, liveAgents: liveAgents).isActive }) {
            return .running
        }
        return .queued
    }

    /// A single card's display status, folding in the live agent's phase and
    /// verdict once the card has actually launched (`taskId` set).
    static func cardStatus(_ card: StackCard, liveAgents: [String: LiveAgent]) -> LoopCardDisplayStatus {
        if let taskId = card.taskId, let agent = liveAgents[taskId] {
            let phase = agent.phase.lowercased()
            if agent.verdictPassed == false {
                return .blocked(reason: "eval failed")
            }
            if phase.contains("failed") || phase.contains("error") {
                return .blocked(reason: agent.phase)
            }
            if agent.active {
                return (phase == "testing" || phase == "scoring" || phase == "verifying") ? .testing : .running
            }
        }
        switch card.status {
        case .done: return .done
        case .running: return .running
        default: return .queued
        }
    }

    /// The pane's most representative goal text for the Overview card's
    /// summary line — the active card's goal if one is running, else the
    /// stack's first card.
    static func representativeGoal(for pane: StackPaneState, liveAgents: [String: LiveAgent]) -> String {
        if let active = pane.cards.first(where: { cardStatus($0, liveAgents: liveAgents).isActive }) {
            return active.goal
        }
        return pane.cards.first?.goal ?? "no cards yet"
    }

    /// Elapsed-time + running cost for the pane's active card, if any.
    static func elapsedAndCost(for pane: StackPaneState, liveAgents: [String: LiveAgent]) -> String? {
        guard let agent = pane.cards.compactMap({ $0.taskId }).compactMap({ liveAgents[$0] }).first(where: \.active)
        else { return nil }
        let elapsed = max(0, Int(Date().timeIntervalSince(agent.startedAt)))
        return String(format: "%dm%02ds · $%.4f", elapsed / 60, elapsed % 60, agent.costUsd)
    }
}

// MARK: - Shared chrome (grammar chips, icon buttons, iteration pill, runtag)

/// A grammar-hint chip (`:alias`, `@repo`, `/model`, `/effort`, `×N`) — an
/// outlined pill in the token's own color, matching the composer's inline
/// grammar row on web/macOS.
struct GrammarChip: View {
    let label: String
    let color: Color

    var body: some View {
        Text(label)
            .font(Konjo.mono(9))
            .foregroundStyle(color)
            .padding(.horizontal, 7)
            .frame(height: 20)
            .overlay(Capsule().stroke(color.opacity(0.4), lineWidth: 1))
    }
}

/// A small square icon button used in the cardbar (facets trigger, duplicate,
/// drag, delete). `count` renders a tiny badge (e.g. the "•••" facet-count
/// indicator).
struct CardIconButton: View {
    let systemImage: String
    var active: Bool = false
    var count: Int?
    var action: () -> Void = {}

    var body: some View {
        Button(action: action) {
            HStack(spacing: 4) {
                Image(systemName: systemImage).font(.system(size: 11))
                if let count {
                    Text("\(count)")
                        .font(Konjo.mono(8, weight: .semibold))
                        .padding(.horizontal, 4)
                        .background(Color.white.opacity(0.12), in: Capsule())
                }
            }
            .foregroundStyle(active ? Konjo.fg : Konjo.fgMute)
            .frame(minWidth: 26, minHeight: 26)
            .padding(.horizontal, count != nil ? 4 : 0)
            .background(active ? Color.white.opacity(0.1) : .clear, in: RoundedRectangle(cornerRadius: 6))
            .overlay(
                RoundedRectangle(cornerRadius: 6)
                    .stroke(active ? Color.white.opacity(0.5) : Konjo.line, lineWidth: 1)
            )
        }
        .buttonStyle(.plain)
    }
}

/// The flame-colored iteration/loop-count pill (`⇄ off` / `⇄ ×N`), always
/// fully visible in the cardbar — never wraps.
struct IterationPill: View {
    let label: String

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: "repeat")
                .font(.system(size: 10, weight: .bold))
            Text(label).font(Konjo.mono(10, weight: .bold))
        }
        .foregroundStyle(Konjo.flame)
        .padding(.horizontal, 8)
        .frame(height: 26)
        .background(Konjo.flame.opacity(0.09), in: RoundedRectangle(cornerRadius: 6))
        .overlay(RoundedRectangle(cornerRadius: 6).stroke(Konjo.flame.opacity(0.5), lineWidth: 1))
    }
}

/// The status-label badge that sits in a notch on a card's top edge
/// ("NEW PROMPT", "BLOCKED", "DONE", ...).
struct RunTag: View {
    let label: String
    let color: Color
    let background: Color

    var body: some View {
        Text(label.uppercased())
            .font(Konjo.mono(8, weight: .bold))
            .tracking(1)
            .foregroundStyle(color)
            .padding(.horizontal, 7)
            .padding(.vertical, 2)
            .background(background, in: RoundedRectangle(cornerRadius: 3))
            .overlay(RoundedRectangle(cornerRadius: 3).stroke(color.opacity(0.5), lineWidth: 1))
    }
}
