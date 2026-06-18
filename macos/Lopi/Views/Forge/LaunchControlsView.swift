import SwiftUI

/// A Konjo-styled dropdown built on `Menu` — accent-aware, mono type, with a
/// checkmark on the active option.
struct KonjoMenu: View {
    var title: String
    var options: [LaunchOption]
    @Binding var value: String
    var dense = false

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            if !dense && !title.isEmpty {
                Text(title)
                    .font(Konjo.mono(8, weight: .semibold))
                    .tracking(1.4)
                    .foregroundStyle(Konjo.fgMute)
            }
            Menu {
                ForEach(options) { opt in
                    Button {
                        value = opt.value
                    } label: {
                        if value == opt.value {
                            Label(opt.label, systemImage: "checkmark")
                        } else {
                            Text(opt.label)
                        }
                    }
                }
            } label: {
                HStack(spacing: 6) {
                    Text(LaunchControls.label(options, value))
                        .font(Konjo.mono(dense ? 10 : 11))
                        .lineLimit(1)
                    Image(systemName: "chevron.down").font(.system(size: 7, weight: .bold))
                }
                .foregroundStyle(Konjo.fg)
                .padding(.horizontal, dense ? 6 : 8)
                .padding(.vertical, dense ? 3 : 5)
                .background(Color.white.opacity(0.025))
                .overlay(RoundedRectangle(cornerRadius: 7).stroke(Konjo.line, lineWidth: 1))
                .clipShape(RoundedRectangle(cornerRadius: 7))
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize()
        }
    }
}

/// A Konjo-styled single-line text field for repo / branch overrides.
private struct KonjoField: View {
    var title: String
    var placeholder: String
    @Binding var value: String
    var dense = false

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            if !dense { Text(title).font(Konjo.mono(8, weight: .semibold)).tracking(1.4).foregroundStyle(Konjo.fgMute) }
            TextField(placeholder, text: $value)
                .textFieldStyle(.plain)
                .font(Konjo.mono(dense ? 10 : 11))
                .foregroundStyle(Konjo.fg)
                .padding(.horizontal, dense ? 6 : 8)
                .padding(.vertical, dense ? 3 : 5)
                .background(Color.white.opacity(0.025))
                .overlay(RoundedRectangle(cornerRadius: 7).stroke(Konjo.line, lineWidth: 1))
                .clipShape(RoundedRectangle(cornerRadius: 7))
                .frame(maxWidth: dense ? 130 : 170)
        }
    }
}

/// The selector cluster — model / effort / priority / repo / branch — bound to
/// the shared, persisted `LaunchControls`.
struct LaunchControlsView: View {
    @Bindable var controls: LaunchControls
    var dense = false

    var body: some View {
        VStack(alignment: .leading, spacing: dense ? 6 : 10) {
            HStack(spacing: 8) {
                KonjoMenu(title: "model", options: LaunchControls.models, value: $controls.model, dense: dense)
                KonjoMenu(title: "effort", options: LaunchControls.efforts, value: $controls.effort, dense: dense)
            }
            HStack(spacing: 8) {
                KonjoMenu(title: "priority", options: LaunchControls.priorities, value: $controls.priority, dense: dense)
            }
            KonjoField(title: "repo", placeholder: "./path or owner/repo", value: $controls.repo, dense: dense)
            KonjoField(title: "branch", placeholder: "auto", value: $controls.branch, dense: dense)
        }
    }
}
