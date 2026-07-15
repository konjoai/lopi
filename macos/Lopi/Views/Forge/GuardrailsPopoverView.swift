import SwiftUI
import LopiStacksKit

/// Shared popover chrome — a dark padded panel with a titled header row, matching
/// the web `Popover` body styling. Native `.popover` supplies positioning +
/// outside-click dismissal (mouse/hover is right for Mac).
struct PopoverChrome<Content: View>: View {
    var systemImage: String
    var title: String
    var accent: Color
    var width: CGFloat = 300
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 7) {
                Image(systemName: systemImage).font(.system(size: 11)).foregroundStyle(accent)
                Text(title).font(Konjo.mono(11, weight: .semibold)).foregroundStyle(Konjo.fg)
            }
            .padding(.horizontal, 13).padding(.top, 12).padding(.bottom, 10)
            Rectangle().fill(Konjo.line).frame(height: 1)
            content.padding(13)
        }
        .frame(width: width)
        .background(Konjo.panel)
    }
}

/// GuardrailsPopover — the sun guardrails button's content. At loop scope every
/// field is WIRED (`gate`/`until`/`on_fail` → real `CreateTaskOptions`); at stack
/// scope gate/until are hidden (no server-side "whole chain" for a shell
/// pre/exit-condition to run against), so showing them would be an inert control
/// — `on_fail` stays wired, driving the chain sequencer. Generalized to value +
/// callback props, exactly like the web component.
struct GuardrailsPopoverView: View {
    enum Scope { case loop, stack }
    var scope: Scope
    var guardrails: Guardrails            // loop scope reads all fields; stack uses onFail/budget
    var maxIterations: Int
    var iterLabel: String = "max iter"
    var onChange: (Guardrails) -> Void
    var onStep: (Int) -> Void

    var body: some View {
        PopoverChrome(systemImage: "shield", title: "guardrails · \(scope == .stack ? "chain limits" : "run limits")", accent: Konjo.sun) {
            VStack(alignment: .leading, spacing: 10) {
                if scope == .loop {
                    gateRow("gate", isOn: guardrails.gate, cmd: guardrails.gateCmd, placeholder: "shell cmd, must pass first") {
                        var g = guardrails; g.gate = !g.gate; onChange(g)
                    } onCmd: { var g = guardrails; g.gateCmd = $0; onChange(g) }
                    gateRow("until", isOn: guardrails.until, cmd: guardrails.untilCmd, placeholder: "loop until exit 0") {
                        var g = guardrails; g.until = !g.until; onChange(g)
                    } onCmd: { var g = guardrails; g.untilCmd = $0; onChange(g) }
                }
                segRow("on fail", options: [(OnFail.stop, "stop"), (.continue, "continue"), (.backoff, "backoff")], selected: guardrails.onFail) {
                    var g = guardrails; g.onFail = $0; onChange(g)
                }
                segRow("budget", options: [(Budget.auto, "auto"), (.k200, "200k"), (.none, "none")], selected: guardrails.budget) {
                    var g = guardrails; g.budget = $0; onChange(g)
                }
                Divider().overlay(Konjo.line)
                HStack {
                    Text(iterLabel.uppercased()).font(Konjo.mono(8.5)).tracking(0.6).foregroundStyle(Konjo.fgDim)
                    stepper
                    Spacer()
                }
            }
        }
    }

    private func gateRow(_ label: String, isOn: Bool, cmd: String, placeholder: String,
                         onToggle: @escaping () -> Void, onCmd: @escaping (String) -> Void) -> some View {
        HStack(spacing: 9) {
            StackToggle(isOn: isOn, accent: Konjo.sun, onToggle: onToggle)
            Text(label).font(Konjo.mono(11)).foregroundStyle(Konjo.fg).frame(width: 38, alignment: .leading)
            TextField(placeholder, text: Binding(get: { cmd }, set: onCmd))
                .textFieldStyle(.plain).font(Konjo.mono(10)).foregroundStyle(Konjo.fg)
                .padding(4).background(Color.white.opacity(0.03))
                .overlay(RoundedRectangle(cornerRadius: 5).stroke(Konjo.line, lineWidth: 1))
                .disabled(!isOn).opacity(isOn ? 1 : 0.35)
        }
    }

    private func segRow<T: Hashable>(_ label: String, options: [(T, String)], selected: T, onSelect: @escaping (T) -> Void) -> some View {
        HStack(spacing: 9) {
            Text(label.uppercased()).font(Konjo.mono(8.5)).tracking(0.6).foregroundStyle(Konjo.fgDim).frame(width: 52, alignment: .leading)
            StackSegmented(options: options, selected: selected, accent: Konjo.sun, onSelect: onSelect)
            Spacer(minLength: 0)
        }
    }

    private var stepper: some View {
        HStack(spacing: 0) {
            Button { onStep(-1) } label: { Text("−").font(Konjo.mono(14)).foregroundStyle(Konjo.sun).frame(width: 24, height: 25) }.buttonStyle(.plain)
            Text(scope == .stack ? maxIterationsLabel(maxIterations) : cardIterationsLabel(maxIterations)).font(Konjo.mono(11)).foregroundStyle(Konjo.fg).frame(width: 34)
                .overlay(Rectangle().fill(Konjo.line).frame(width: 1), alignment: .leading)
                .overlay(Rectangle().fill(Konjo.line).frame(width: 1), alignment: .trailing)
            Button { onStep(1) } label: { Text("+").font(Konjo.mono(14)).foregroundStyle(Konjo.sun).frame(width: 24, height: 25) }.buttonStyle(.plain)
        }
        .overlay(RoundedRectangle(cornerRadius: 6).stroke(Konjo.line, lineWidth: 1))
        .clipShape(RoundedRectangle(cornerRadius: 6))
    }
}
