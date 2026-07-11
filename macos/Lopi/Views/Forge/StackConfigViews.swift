import SwiftUI

/// A labeled Konjo dropdown over `StackOption`s — the shared primitive both the
/// stack-defaults popover and the per-loop config drawer mount (not a fork).
struct StackDropdown: View {
    var label: String
    var value: String
    var options: [StackOption]
    var accent: Color = Konjo.ice
    var onSelect: (String) -> Void

    private var currentLabel: String {
        options.first { $0.value == value }?.label ?? (value.isEmpty ? "auto" : value)
    }

    var body: some View {
        HStack(spacing: 8) {
            Text(label).font(Konjo.mono(9, weight: .semibold)).tracking(0.8).foregroundStyle(Konjo.fgDim).frame(width: 62, alignment: .leading)
            Menu {
                ForEach(options, id: \.value) { opt in
                    Button {
                        onSelect(opt.value)
                    } label: {
                        if opt.value == value { Label(opt.label, systemImage: "checkmark") } else { Text(opt.label) }
                    }
                }
            } label: {
                HStack(spacing: 6) {
                    Text(currentLabel).font(Konjo.mono(12)).lineLimit(1)
                    Image(systemName: "chevron.down").font(.system(size: 8, weight: .bold))
                }
                .foregroundStyle(Konjo.fg)
                .padding(.horizontal, 8).padding(.vertical, 4)
                .overlay(RoundedRectangle(cornerRadius: 7).stroke(Konjo.line, lineWidth: 1))
            }
            .menuStyle(.borderlessButton).menuIndicator(.hidden).fixedSize()
        }
    }
}

/// StackConfigPopover — the dock's sliders button: the stack's own default
/// model/effort/repo/branch/autonomy (every loop inherits these). `model`/
/// `effort`/`repo` are WIRED; `branch`/`autonomy` are client-only.
struct StackConfigPopoverView: View {
    var defaults: StackDefaults
    var repoOptions: [StackOption]
    var onChange: (StackDefaults) -> Void

    private var effectiveRepoOptions: [StackOption] {
        repoOptions.isEmpty ? [StackOption(value: defaults.repo, label: defaults.repo.isEmpty ? "auto" : defaults.repo)] : repoOptions
    }

    var body: some View {
        PopoverChrome(systemImage: "slider.horizontal.3", title: "default config · every loop inherits", accent: Konjo.stackViolet) {
            VStack(alignment: .leading, spacing: 9) {
                StackDropdown(label: "model", value: defaults.model, options: MODEL_OPTIONS) { v in onChange({ var d = defaults; d.model = v; return d }()) }
                StackDropdown(label: "effort", value: defaults.effort, options: EFFORT_OPTIONS) { v in onChange({ var d = defaults; d.effort = v; return d }()) }
                StackDropdown(label: "repo", value: defaults.repo, options: effectiveRepoOptions) { v in onChange({ var d = defaults; d.repo = v; return d }()) }
                StackDropdown(label: "branch", value: defaults.branch, options: BRANCH_OPTIONS) { v in onChange({ var d = defaults; d.branch = v; return d }()) }
                StackDropdown(label: "autonomy", value: defaults.autonomy, options: AUTONOMY_OPTIONS) { v in onChange({ var d = defaults; d.autonomy = v; return d }()) }
            }
        }
    }
}

/// ConfigDrawer — the per-loop sliders inline drawer: five overrides of the pane
/// defaults. `nil`-valued overrides fall back to the pane default in the display.
struct ConfigDrawerView: View {
    var config: CardConfig
    var paneDefaults: StackDefaults
    var repoOptions: [StackOption]
    var onChange: (CardConfig) -> Void

    private var effectiveRepoOptions: [StackOption] {
        repoOptions.isEmpty ? [StackOption(value: paneDefaults.repo, label: paneDefaults.repo.isEmpty ? "auto" : paneDefaults.repo)] : repoOptions
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            StackDropdown(label: "model", value: config.model ?? paneDefaults.model, options: MODEL_OPTIONS) { v in onChange({ var c = config; c.model = v; return c }()) }
            StackDropdown(label: "effort", value: config.effort ?? paneDefaults.effort, options: EFFORT_OPTIONS) { v in onChange({ var c = config; c.effort = v; return c }()) }
            StackDropdown(label: "repo", value: config.repo ?? paneDefaults.repo, options: effectiveRepoOptions) { v in onChange({ var c = config; c.repo = v; return c }()) }
            StackDropdown(label: "branch", value: config.branch ?? paneDefaults.branch, options: BRANCH_OPTIONS) { v in onChange({ var c = config; c.branch = v; return c }()) }
            StackDropdown(label: "autonomy", value: config.autonomy ?? paneDefaults.autonomy, options: AUTONOMY_OPTIONS) { v in onChange({ var c = config; c.autonomy = v; return c }()) }
        }
        .padding(.top, 12)
        .overlay(Rectangle().fill(Konjo.line).frame(height: 1), alignment: .top)
    }
}
