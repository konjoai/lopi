import SwiftUI
import LopiStacksKit

/// EvalsPopover — the jade evals button's content. Client-only intent (eval
/// execution doesn't exist server-side yet at either scope): an editor of which
/// checks *would* run, never a pass/fail state. Baseline is locked-on.
/// Generalized to an `evals` value + `onChange`, so the same view mounts scoped
/// to one loop or to the whole stack ("chain acceptance").
struct EvalsPopoverView: View {
    var evals: [EvalRef]
    var heading: String = "loop validation"
    var onChange: ([EvalRef]) -> Void

    private var suiteKeys: [String] { Array(EVAL_SUITES.keys).sorted() }

    private func isOn(_ name: String) -> Bool { evals.contains { $0.name == name } }

    var body: some View {
        PopoverChrome(systemImage: "checkmark.square", title: "evals · \(heading)", accent: Konjo.jade) {
            VStack(alignment: .leading, spacing: 2) {
                ForEach(EVAL_CATALOG, id: \.name) { e in
                    row(e)
                }
                Divider().overlay(Konjo.line).padding(.vertical, 6)
                HStack(spacing: 7) {
                    Text("suite:").font(Konjo.mono(10)).foregroundStyle(Konjo.fgDim)
                    ForEach(suiteKeys, id: \.self) { key in
                        Button { onChange(applySuite(evals, EVAL_SUITES[key] ?? [])) } label: {
                            Text(key).font(Konjo.mono(10))
                                .foregroundStyle(key == "kcqf" ? Konjo.sun : Konjo.fg)
                                .padding(.horizontal, 10).padding(.vertical, 3)
                                .overlay(RoundedRectangle(cornerRadius: 11)
                                    .strokeBorder(key == "kcqf" ? Konjo.sun.opacity(0.4) : Konjo.line2,
                                                  style: StrokeStyle(lineWidth: 1, dash: [3])))
                        }
                        .buttonStyle(.plain)
                    }
                    Spacer(minLength: 0)
                }
            }
        }
    }

    private func row(_ e: EvalRef) -> some View {
        let locked = e.name == BASELINE_EVAL.name
        let on = isOn(e.name)
        return Button {
            if !locked { onChange(toggleEval(evals, e.name)) }
        } label: {
            HStack(spacing: 9) {
                RoundedRectangle(cornerRadius: 4)
                    .fill(on ? Konjo.jade : Color.clear)
                    .frame(width: 16, height: 16)
                    .overlay(RoundedRectangle(cornerRadius: 4).stroke(on ? Konjo.jade : Konjo.line2, lineWidth: 1.5))
                    .overlay(Image(systemName: "checkmark").font(.system(size: 9, weight: .bold)).foregroundStyle(Konjo.bg).opacity(on ? 1 : 0))
                Text(e.name).font(Konjo.mono(11.5)).foregroundStyle(on ? Konjo.fg : Konjo.fgDim)
                Spacer(minLength: 0)
                Text(e.tier.rawValue.uppercased()).font(Konjo.mono(8)).tracking(0.6)
                    .foregroundStyle(tierColor(e.tier))
                    .padding(.horizontal, 7).padding(.vertical, 1)
                    .overlay(RoundedRectangle(cornerRadius: 10).stroke(tierColor(e.tier).opacity(0.3), lineWidth: 1))
            }
            .padding(.vertical, 5).padding(.horizontal, 6)
            .opacity(locked ? 0.6 : 1)
        }
        .buttonStyle(.plain)
        .disabled(locked)
    }

    private func tierColor(_ tier: EvalTier) -> Color {
        switch tier {
        case .base: return Konjo.jade
        case .test: return Konjo.ice
        case .judge: return Konjo.stackViolet
        case .suite: return Konjo.sun
        }
    }
}
