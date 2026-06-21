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
                    .font(Konjo.mono(10, weight: .semibold))
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
                        .font(Konjo.mono(dense ? 12.5 : 13.75))
                        .lineLimit(1)
                    Image(systemName: "chevron.down").font(.system(size: 9, weight: .bold))
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

/// The selector cluster — model / effort / priority / repo / branch — bound to
/// the shared, persisted `LaunchControls`.
struct LaunchControlsView: View {
    @Environment(AppModel.self) private var model
    @Bindable var controls: LaunchControls
    var dense = false

    /// Repo dropdown options — server-discovered git repos (shown by basename),
    /// with a leading "no override" entry.
    private var repoOptions: [LaunchOption] {
        [LaunchOption(value: "", label: "— repo —")]
            + model.repos.map { LaunchOption(value: $0, label: ($0 as NSString).lastPathComponent) }
    }

    /// Branch dropdown options — branches of the selected repo, "auto" leading.
    private var branchOptions: [LaunchOption] {
        [LaunchOption(value: "", label: "auto")]
            + model.branches.map { LaunchOption(value: $0, label: $0) }
    }

    var body: some View {
        HStack(spacing: 8) {
            KonjoMenu(title: "model", options: LaunchControls.models, value: $controls.model, dense: dense)
            KonjoMenu(title: "effort", options: LaunchControls.efforts, value: $controls.effort, dense: dense)
            KonjoMenu(title: "priority", options: LaunchControls.priorities, value: $controls.priority, dense: dense)
            KonjoMenu(title: "repo", options: repoOptions, value: $controls.repo, dense: dense)
            KonjoMenu(title: "branch", options: branchOptions, value: $controls.branch, dense: dense)
        }
        .task {
            await model.refreshRepos()
            await model.refreshBranches(controls.repo)
        }
        .onChange(of: controls.repo) { _, newRepo in
            controls.branch = ""
            Task { await model.refreshBranches(newRepo) }
        }
    }
}
