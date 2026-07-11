import Foundation

// The card's orb-state resolver — the macOS analogue of web's
// `$lib/forge/cardOrb.ts::orbStateForCard`. A stack card's sole status
// vocabulary is its orb, driven by the live agent keyed by `card.taskId` through
// the exact same `OrbStateMap.compute` the Forge pane uses — so color/motion mean
// identically what they mean there. A card that hasn't launched (no `taskId`, or
// no live agent yet) shows the calm idle orb.
enum CardOrb {
    @MainActor
    static func state(for taskId: String?, in liveAgents: [String: LiveAgent]) -> ForgeOrbState {
        guard let taskId, let agent = liveAgents[taskId] else { return .idle }
        return OrbStateMap.compute(agent, awaiting: agent.awaitingApproval)
    }

    /// A hover/accessibility label for the card orb — mirrors web's `orbLabel`.
    static func label(for card: StackCard) -> String {
        if card.status == .running, let it = card.iteration {
            return "running · iter \(it.current)/\(it.total)"
        }
        return card.status.rawValue
    }
}
