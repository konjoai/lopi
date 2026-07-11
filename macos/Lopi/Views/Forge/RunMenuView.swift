import SwiftUI

/// RunMenu — the run-stack chevron's dropdown. Genuinely wired to the
/// `StackRunEngine`: Run now / Run once / Schedule stack / Dry run when no run is
/// active, or Pause/Resume + Drain once one is. Dry run stays available in both
/// states (it never touches execution).
struct RunMenuView: View {
    var store: StackStore
    var engine: StackRunEngine
    var paneKey: String
    var defaults: PaneDefaults
    var phase: RunPhase?
    var onDryRun: (DryRunResult) -> Void
    var onClose: () -> Void

    private struct Item: Identifiable {
        let id = UUID()
        let systemImage: String
        let name: String
        let sub: String
        let action: () -> Void
    }

    var body: some View {
        VStack(spacing: 0) {
            ForEach(items) { it in
                Button { it.action(); onClose() } label: {
                    HStack(spacing: 13) {
                        Image(systemName: it.systemImage).font(.system(size: 14)).foregroundStyle(Konjo.flame).frame(width: 16)
                        Text(it.name).font(Konjo.sans(14)).foregroundStyle(Konjo.fg)
                        Spacer()
                        Text(it.sub).font(Konjo.mono(10)).foregroundStyle(Konjo.fgMute)
                    }
                    .padding(.horizontal, 17).padding(.vertical, 13)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                Divider().overlay(Konjo.line)
            }
        }
        .frame(width: 320)
        .background(Konjo.panel)
    }

    private var items: [Item] {
        var out: [Item] = []
        if phase == .running {
            out.append(Item(systemImage: "pause.fill", name: "Pause", sub: "halt after this card") { engine.pauseStack(paneKey) })
            out.append(Item(systemImage: "xmark", name: "Drain", sub: "finish then stop") { engine.drainStack(paneKey) })
        } else if phase == .paused {
            out.append(Item(systemImage: "play.fill", name: "Resume", sub: "continue run") { engine.resumeStack(paneKey, defaults) })
            out.append(Item(systemImage: "xmark", name: "Drain", sub: "stop for good") { engine.drainStack(paneKey) })
        } else {
            out.append(Item(systemImage: "play.fill", name: "Run now", sub: "start now") { engine.runStack(paneKey, .run, defaults) })
            out.append(Item(systemImage: "checkmark", name: "Run once", sub: "one pass each") { engine.runStack(paneKey, .runOnce, defaults) })
            out.append(Item(systemImage: "clock", name: "Schedule stack", sub: "one cron, bottom card") { scheduleStack() })
        }
        out.append(Item(systemImage: "testtube.2", name: "Dry run", sub: "validate only") { dryRun() })
        return out
    }

    private func dryRun() {
        let cards = store.pane(for: paneKey)?.cards ?? []
        onDryRun(dryRunStack(cards, defaults))
    }

    private func scheduleStack() {
        let cards = executionOrder(store.pane(for: paneKey)?.cards ?? [])
        guard let first = cards.first else { return }
        let cronExpr = buildCronString(first.cron)
        Task { _ = await engine.scheduleStack(paneKey, cronExpr, defaults) }
    }
}
