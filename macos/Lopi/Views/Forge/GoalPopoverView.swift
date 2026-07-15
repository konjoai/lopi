import SwiftUI
import LopiStacksKit

/// GoalPopoverView — the flame gauge/goal button's content. Explains what
/// "pursue goal" actually does (the toggle used to be a bare, unconfigurable
/// button) and exposes `StackGoal.noProgressLimit`, a field that already
/// existed on the model but had no UI writer anywhere until now. Mirrors the
/// web `GoalPopover.svelte` verbatim.
struct GoalPopoverView: View {
    var pursue: Bool
    var noProgressLimit: Int
    /// True once `pursue` is on *and* the stack carries real chain-acceptance
    /// evals beyond the baseline — mirrors `stackPursuesGoal`.
    var pursues: Bool
    var onTogglePursue: () -> Void
    var onChangeNoProgressLimit: (Int) -> Void

    var body: some View {
        PopoverChrome(systemImage: "gauge", title: "goal", accent: Konjo.flame) {
            VStack(alignment: .leading, spacing: 10) {
                Text("When on, the stack re-runs its whole chain of loops until the chain-acceptance evals pass — \"pursue goal\" instead of a single \"run stack\".")
                    .font(Konjo.mono(10)).foregroundStyle(Konjo.fgDim)
                HStack(spacing: 9) {
                    StackToggle(isOn: pursue, accent: Konjo.flame, onToggle: onTogglePursue)
                    Text("pursue").font(Konjo.mono(11)).foregroundStyle(Konjo.fg)
                }
                if pursue && !pursues {
                    Text("add chain-acceptance evals for the goal to pursue — a goal with nothing to check is inert")
                        .font(Konjo.mono(9)).foregroundStyle(Konjo.fgMute)
                }
                HStack {
                    Text("NO-PROGRESS LIMIT").font(Konjo.mono(8.5)).tracking(0.6).foregroundStyle(Konjo.fgDim)
                    stepper
                    Spacer()
                }
                Text("stop after this many consecutive chain-runs with no gain; 0 disables the no-progress check.")
                    .font(Konjo.mono(9)).foregroundStyle(Konjo.fgMute)
            }
        }
    }

    private var stepper: some View {
        HStack(spacing: 0) {
            Button { onChangeNoProgressLimit(max(0, noProgressLimit - 1)) } label: {
                Text("−").font(Konjo.mono(14)).foregroundStyle(Konjo.flame).frame(width: 24, height: 25)
            }.buttonStyle(.plain)
            Text(noProgressLimit == 0 ? "off" : String(noProgressLimit))
                .font(Konjo.mono(11)).foregroundStyle(Konjo.fg).frame(width: 34)
                .overlay(Rectangle().fill(Konjo.line).frame(width: 1), alignment: .leading)
                .overlay(Rectangle().fill(Konjo.line).frame(width: 1), alignment: .trailing)
            Button { onChangeNoProgressLimit(noProgressLimit + 1) } label: {
                Text("+").font(Konjo.mono(14)).foregroundStyle(Konjo.flame).frame(width: 24, height: 25)
            }.buttonStyle(.plain)
        }
        .overlay(RoundedRectangle(cornerRadius: 6).stroke(Konjo.line, lineWidth: 1))
        .clipShape(RoundedRectangle(cornerRadius: 6))
    }
}
