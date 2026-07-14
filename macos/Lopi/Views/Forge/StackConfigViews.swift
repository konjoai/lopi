import SwiftUI

/// The mockup's `.cfgchip` — `.kdrop-trigger.chip` in `Dropdown.svelte`: a
/// horizontal `icon · LABEL · value ⌄` pill with the icon tinted `accent`.
///
/// Its own view because two controls mount it: `StackDropdown` (a native `Menu`,
/// for the short catalogs) and `RepoPickerView` (a popover, because a `Menu`
/// can't hold a search field). Only the *presentation* forks — the chip itself
/// must stay pixel-identical across all five fields of the drawer.
struct ConfigChip: View {
    var label: String
    var text: String
    var icon: String
    var accent: Color

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: icon).font(.system(size: 12)).foregroundStyle(accent)
            Text(label.uppercased()).font(Konjo.mono(8)).tracking(0.4).foregroundStyle(Konjo.fg.opacity(0.5))
            // `lineLimit` alone won't shrink a chip: `FlowLayout` places every
            // subview at its ideal size, so a long label just makes a wide chip.
            // The cap is what makes the ellipsis real — matching web's
            // `.kdrop-value { max-width: 13rem }`.
            Text(text).font(Konjo.mono(11)).foregroundStyle(Konjo.fg)
                .lineLimit(1).truncationMode(.tail).frame(maxWidth: 208, alignment: .leading)
                .fixedSize(horizontal: false, vertical: true)
            Image(systemName: "chevron.down").font(.system(size: 9)).foregroundStyle(Konjo.fg.opacity(0.5))
        }
        .padding(.horizontal, 11).padding(.vertical, 7)
        .background(Color.white.opacity(0.025))
        .overlay(RoundedRectangle(cornerRadius: 7).stroke(Konjo.line, lineWidth: 1))
        .clipShape(RoundedRectangle(cornerRadius: 7))
    }
}

/// A labeled Konjo dropdown over `StackOption`s — the shared primitive both the
/// stack-defaults popover and the per-loop config drawer mount (not a fork).
struct StackDropdown: View {
    var label: String
    var value: String
    var options: [StackOption]
    var accent: Color = Konjo.ice
    /// Optional leading SF Symbol. When set, the whole control renders as the
    /// mockup's horizontal `icon · LABEL · value ⌄` chip (icon tinted `accent`);
    /// when nil it keeps the plain label-column form.
    var icon: String? = nil
    var onSelect: (String) -> Void

    private var currentLabel: String {
        options.first { $0.value == value }?.label ?? (value.isEmpty ? "auto" : value)
    }

    var body: some View {
        if let icon {
            menu { ConfigChip(label: label, text: currentLabel, icon: icon, accent: accent) }
        } else {
            HStack(spacing: 8) {
                Text(label).font(Konjo.mono(9, weight: .semibold)).tracking(0.8).foregroundStyle(Konjo.fgDim).frame(width: 62, alignment: .leading)
                menu {
                    HStack(spacing: 6) {
                        Text(currentLabel).font(Konjo.mono(12)).lineLimit(1)
                        Image(systemName: "chevron.down").font(.system(size: 8, weight: .bold))
                    }
                    .foregroundStyle(Konjo.fg)
                    .padding(.horizontal, 8).padding(.vertical, 4)
                    .background(Color.white.opacity(0.025))
                    .overlay(RoundedRectangle(cornerRadius: 7).stroke(Konjo.line, lineWidth: 1))
                    .clipShape(RoundedRectangle(cornerRadius: 7))
                }
            }
        }
    }

    /// The chip, as a `Menu` rendered through a `ButtonStyle`.
    ///
    /// `.menuStyle(.borderlessButton)` cannot be used here: that style is backed
    /// by an AppKit pull-down which does not render its label as a view tree —
    /// it extracts a *title* and an *image* and discards everything else, both
    /// sibling `Text`/`Image` views and every modifier applied inside the
    /// closure. The chip passed as a label therefore rendered only its icon and
    /// `MODEL`, silently dropping the value, the chevron, the padding and the
    /// border. `.menuStyle(.button)` instead routes the label through the
    /// ambient `ButtonStyle`, whose `configuration.label` is the real SwiftUI
    /// view — so the chip draws in full and still opens the native menu.
    ///
    /// (Overlaying a transparent `Menu` over a plain chip also renders, but is
    /// dead on click: SwiftUI keeps hit-testing at `.opacity(0)`, while the
    /// AppKit control behind the menu does not.)
    private func menu<Label: View>(@ViewBuilder label triggerLabel: () -> Label) -> some View {
        Menu {
            ForEach(options, id: \.value) { opt in
                Button { onSelect(opt.value) } label: {
                    if opt.value == value { SwiftUI.Label(opt.label, systemImage: "checkmark") } else { Text(opt.label) }
                }
            }
        } label: {
            triggerLabel()
        }
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .fixedSize()
    }
}

/// The selected repo's branches as dropdown options, plus the branch to show
/// for them. Shared by the two config surfaces so a stack default and a loop
/// override resolve a repo switch identically.
private struct BranchBinding {
    var options: [StackOption]
    var resolved: String
}

@MainActor private func branchBinding(model: AppModel, repo: String, current: String) -> BranchBinding {
    let branches = model.branchesByRepo[repo] ?? []
    return BranchBinding(
        options: branches.map { StackOption(value: $0, label: $0) },
        resolved: resolveBranch(current, branches, model.headBranchByRepo[repo] ?? "")
    )
}

/// StackConfigPopover — the dock's sliders button: the stack's own default
/// model/effort/repo/branch/autonomy (every loop inherits these). `model`/
/// `effort`/`repo` are WIRED; `autonomy` is client-only; `branch` reaches the
/// server as a planning constraint and lists the selected repo's real branches.
struct StackConfigPopoverView: View {
    @Environment(AppModel.self) private var model
    var defaults: StackDefaults
    var repoOptions: [StackOption]
    var onChange: (StackDefaults) -> Void

    private var effectiveRepoOptions: [StackOption] {
        repoOptions.isEmpty ? [StackOption(value: defaults.repo, label: defaults.repo.isEmpty ? "auto" : defaults.repo)] : repoOptions
    }

    private var branch: BranchBinding { branchBinding(model: model, repo: defaults.repo, current: defaults.branch) }

    /// Store what we show. Leaving a stale branch in `defaults` while displaying
    /// a resolved one would launch against a target the user never saw.
    private func syncBranch() {
        guard branch.resolved != defaults.branch else { return }
        onChange({ var d = defaults; d.branch = branch.resolved; return d }())
    }

    var body: some View {
        PopoverChrome(systemImage: "slider.horizontal.3", title: "default config · every loop inherits", accent: Konjo.stackViolet) {
            VStack(alignment: .leading, spacing: 9) {
                StackDropdown(label: "model", value: defaults.model, options: MODEL_OPTIONS, accent: Konjo.ice, icon: "cpu") { v in onChange({ var d = defaults; d.model = v; return d }()) }
                StackDropdown(label: "effort", value: defaults.effort, options: EFFORT_OPTIONS, accent: Konjo.ember, icon: "gauge.medium") { v in onChange({ var d = defaults; d.effort = v; return d }()) }
                RepoPickerView(label: "repo", value: defaults.repo, options: effectiveRepoOptions) { v in onChange({ var d = defaults; d.repo = v; return d }()) }
                StackDropdown(label: "branch", value: branch.resolved, options: branch.options, accent: Konjo.jade, icon: "arrow.triangle.branch") { v in onChange({ var d = defaults; d.branch = v; return d }()) }
                StackDropdown(label: "autonomy", value: defaults.autonomy, options: AUTONOMY_OPTIONS, accent: Konjo.stackViolet, icon: "square.stack.3d.up") { v in onChange({ var d = defaults; d.autonomy = v; return d }()) }
            }
        }
        .task(id: defaults.repo) {
            await model.ensureBranches(defaults.repo)
            syncBranch()
        }
    }
}

/// ConfigDrawer — the per-loop sliders inline drawer: five overrides of the pane
/// defaults. `nil`-valued overrides fall back to the pane default in the display.
struct ConfigDrawerView: View {
    @Environment(AppModel.self) private var model
    var config: CardConfig
    var paneDefaults: StackDefaults
    var repoOptions: [StackOption]
    var onChange: (CardConfig) -> Void

    private var effectiveRepoOptions: [StackOption] {
        repoOptions.isEmpty ? [StackOption(value: paneDefaults.repo, label: paneDefaults.repo.isEmpty ? "auto" : paneDefaults.repo)] : repoOptions
    }

    /// This card's own repo — not the pane's — drives its branch list.
    private var repo: String { config.repo ?? paneDefaults.repo }
    private var branch: BranchBinding {
        branchBinding(model: model, repo: repo, current: config.branch ?? paneDefaults.branch)
    }

    /// Store what we show — see `StackConfigPopoverView.syncBranch`.
    private func syncBranch() {
        guard branch.resolved != (config.branch ?? paneDefaults.branch) else { return }
        onChange({ var c = config; c.branch = branch.resolved; return c }())
    }

    var body: some View {
        FlowLayout(hSpacing: 6, vSpacing: 6) {
            StackDropdown(label: "model", value: config.model ?? paneDefaults.model, options: MODEL_OPTIONS, accent: Konjo.ice, icon: "cpu") { v in onChange({ var c = config; c.model = v; return c }()) }
            StackDropdown(label: "effort", value: config.effort ?? paneDefaults.effort, options: EFFORT_OPTIONS, accent: Konjo.ember, icon: "gauge.medium") { v in onChange({ var c = config; c.effort = v; return c }()) }
            RepoPickerView(label: "repo", value: repo, options: effectiveRepoOptions) { v in onChange({ var c = config; c.repo = v; return c }()) }
            StackDropdown(label: "branch", value: branch.resolved, options: branch.options, accent: Konjo.jade, icon: "arrow.triangle.branch") { v in onChange({ var c = config; c.branch = v; return c }()) }
            StackDropdown(label: "autonomy", value: config.autonomy ?? paneDefaults.autonomy, options: AUTONOMY_OPTIONS, accent: Konjo.stackViolet, icon: "square.stack.3d.up") { v in onChange({ var c = config; c.autonomy = v; return c }()) }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.top, 12)
        .overlay(Rectangle().fill(Color.white.opacity(0.05)).frame(height: 1), alignment: .top)
        .padding(.top, 12)
        .task(id: repo) {
            await model.ensureBranches(repo)
            syncBranch()
        }
    }
}
