import Foundation

// Pure card + eval + iteration + predicate ops — the port of the tested pure
// functions in `stores/stack.ts`. Total and side-effect-free; unit-tested by
// `StackStoreTests` exactly as the web `stack.test.ts` tests them. Foundation
// only, no UI import.

// MARK: - Budget

/// Resolve a budget preset to the enforced per-loop token cap, or `nil` when the
/// preset sets no hard cap (`auto` inherits, `none` is uncapped — both omit the
/// field so the payload never claims a limit the loop won't enforce).
public func budgetToTokens(_ budget: Budget) -> Int? {
    budget == .k200 ? 200_000 : nil
}

// MARK: - Preset resolution / suggestion

private func isPresetKey(_ s: String) -> Bool {
    PresetKey(rawValue: s) != nil
}

/// Resolve a raw alias token (without the leading `:`) to a preset key, applying
/// legacy renames. Returns `nil` when it names no known preset.
public func resolvePresetAlias(_ alias: String) -> PresetKey? {
    if let key = PresetKey(rawValue: alias) { return key }
    return LEGACY_ALIASES[alias]
}

/// One alias-autocomplete candidate — the full token (leading colon included),
/// ready to write straight into the goal field.
public struct AliasSuggestion: Equatable {
    public let alias: String
    public let label: String
    public let hint: String
}

/// Filtered alias suggestions for the goal field's autocomplete, given its
/// *entire current value*. Only suggests while the field is still a bare
/// `:token` with no space yet — once a space follows, the goal text has moved
/// on and this returns `[]`. Only canonical `PRESET_KEYS` are ever suggested;
/// legacy aliases (e.g. the renamed `:ratchet`→`:gain`) still resolve on
/// commit but never appear here. Mirrors the web `aliasAutocomplete` verbatim.
public func aliasAutocomplete(_ goalText: String) -> [AliasSuggestion] {
    guard goalText.hasPrefix(":"), !goalText.dropFirst().contains(where: { $0.isWhitespace }) else { return [] }
    let query = goalText.dropFirst().lowercased()
    return PRESET_KEYS
        .filter { $0.rawValue.lowercased().hasPrefix(query) }
        .compactMap { key -> AliasSuggestion? in
            guard let def = PRESET_CATALOG[key], let hint = PRESET_DESCRIPTIONS[key] else { return nil }
            return AliasSuggestion(alias: def.alias, label: def.label, hint: hint)
        }
}

/// Keyword-match a typed goal against the preset catalog. Highlight-only —
/// callers must never auto-attach the result. Returns the first matching preset,
/// or `nil`.
public func suggestPreset(_ text: String) -> PresetKey? {
    let lower = text.lowercased()
    for key in PRESET_KEYS {
        if let def = PRESET_CATALOG[key], def.keywords.contains(where: { lower.contains($0) }) {
            return key
        }
    }
    return nil
}

// MARK: - Inline `/command` autocomplete
// Every prompt/stack setting gets a `:`/`@`/`/` alias, not just presets and
// repo: `/model`, `/effort`, `/branch`, `/autonomy`, `/eval` are value-pickers
// (mirrors the user's own suggested `/model/<autocomplete>` syntax — the
// level-2 token embeds the real value directly, so unlike `@repo` there's no
// label/path resolution step); `/guard`, `/schedule`, `/maxx`, `/goal` carry
// multi-field state that doesn't reduce to one inline value, so picking one
// just opens the existing popover for it (the composer view owns that action
// — this module only supplies the pure matching). 1:1 port of the web
// `commandAutocomplete`/`commandValueAutocomplete` pair.

/// One inline `/command` definition.
public struct InlineCommandDef: Equatable {
    public let command: String
    public let hint: String
    /// `true` → typing `/command` then continues into a second
    /// `/command/value` token (see `commandValueAutocomplete`). `false` →
    /// selecting the command fires an immediate action (open a popover) with
    /// no value step.
    public let isValuePicker: Bool

    public init(command: String, hint: String, isValuePicker: Bool) {
        self.command = command
        self.hint = hint
        self.isValuePicker = isValuePicker
    }
}

/// Card-scope commands, typed into a loop's own goal field. No `maxx` here —
/// macOS's `StackCard` has no MAXX field yet (web-only feature to date).
public let CARD_COMMANDS: [InlineCommandDef] = [
    InlineCommandDef(command: "model", hint: "override this loop's model", isValuePicker: true),
    InlineCommandDef(command: "effort", hint: "override this loop's effort", isValuePicker: true),
    InlineCommandDef(command: "branch", hint: "override this loop's target branch", isValuePicker: true),
    InlineCommandDef(command: "autonomy", hint: "override this loop's autonomy level", isValuePicker: true),
    InlineCommandDef(command: "eval", hint: "toggle an eval suite (kcqf/security/research)", isValuePicker: true),
    InlineCommandDef(command: "guard", hint: "open this loop's guardrails", isValuePicker: false),
    InlineCommandDef(command: "schedule", hint: "open this loop's schedule", isValuePicker: false)
]

/// Stack-scope commands, typed into the stack's own command bar
/// (`StackControlDockView`) — same vocabulary, writes to `pane.config`
/// instead of a card's `config`. Adds `loop` (chain loop count) and `goal`
/// (run-until-goal), which have no card-level analog.
public let STACK_COMMANDS: [InlineCommandDef] = [
    InlineCommandDef(command: "model", hint: "stack default model", isValuePicker: true),
    InlineCommandDef(command: "effort", hint: "stack default effort", isValuePicker: true),
    InlineCommandDef(command: "branch", hint: "stack default branch", isValuePicker: true),
    InlineCommandDef(command: "autonomy", hint: "stack default autonomy", isValuePicker: true),
    InlineCommandDef(command: "loop", hint: "stack loop count", isValuePicker: true),
    InlineCommandDef(command: "eval", hint: "toggle a stack eval suite", isValuePicker: true),
    InlineCommandDef(command: "guard", hint: "open stack guardrails", isValuePicker: false),
    InlineCommandDef(command: "schedule", hint: "open the stack schedule", isValuePicker: false),
    InlineCommandDef(command: "goal", hint: "open run-until-goal", isValuePicker: false)
]

/// A level-1 `/command` suggestion — the bare command name, not yet a value.
public struct CommandSuggestion: Equatable {
    public let token: String
    public let command: String
    public let label: String
    public let hint: String
}

/// Level 1: filtered command-name suggestions for a trailing `/token` — the
/// same trailing-word grammar `repoAutocomplete` uses, generalized over a
/// caller-supplied command list (card vs. stack scope differ).
public func commandAutocomplete(_ goalText: String, _ commands: [InlineCommandDef]) -> [CommandSuggestion] {
    guard let slashIndex = goalText.lastIndex(of: "/") else { return [] }
    let isWordStart = slashIndex == goalText.startIndex || goalText[goalText.index(before: slashIndex)].isWhitespace
    guard isWordStart else { return [] }
    let after = goalText[goalText.index(after: slashIndex)...]
    guard after.allSatisfy({ $0.isLowercase || $0.isNumber }) else { return [] }
    let query = after.lowercased()
    return commands
        .filter { $0.command.hasPrefix(query) }
        .map { CommandSuggestion(token: "/\($0.command)", command: $0.command, label: $0.command, hint: $0.hint) }
}

/// A level-2 `/command/value` suggestion.
public struct CommandValueSuggestion: Equatable {
    public let token: String
    public let label: String
    public let hint: String
    public let value: String
}

/// Level 2: once a value-picker command has been chosen (the composer view
/// tracks this as its own `pendingCommand` state), matches a trailing
/// `/command/value` token against whatever catalog applies to `command`.
public func commandValueAutocomplete(_ goalText: String, _ command: String, _ options: [StackOption]) -> [CommandValueSuggestion] {
    let prefix = "/\(command)/"
    guard let range = goalText.range(of: prefix, options: .backwards) else { return [] }
    let isWordStart = range.lowerBound == goalText.startIndex || goalText[goalText.index(before: range.lowerBound)].isWhitespace
    guard isWordStart else { return [] }
    let after = goalText[range.upperBound...]
    guard !after.contains(where: { $0.isWhitespace }) else { return [] }
    let query = after.lowercased()
    return options
        .filter { $0.value != "" && optionMatches($0, query) }
        .map { CommandValueSuggestion(token: "\(prefix)\($0.value)", label: $0.label, hint: $0.hint, value: $0.value) }
}

/// `/eval`'s value catalog is the suite-shortcut names (`kcqf`/`security`/
/// `research`), not individual eval names — those contain spaces (`"vuln
/// scan"`, `"code review"`), which the trailing-token grammar can't carry.
/// Bulk-toggling a suite is the useful, space-free case; per-eval toggling
/// stays a popover click.
public func evalSuiteOptions() -> [StackOption] {
    EVAL_SUITES.keys.sorted().map { StackOption(value: $0, label: $0) }
}

// MARK: - Composer grammar parser

/// The pieces a composer/CLI string parses into.
public struct ParsedInput: Equatable {
    public var alias: String?
    public var goal: String
    public var repo: String?
    public var loopN: Int?

    public init(alias: String?, goal: String, repo: String?, loopN: Int?) {
        self.alias = alias
        self.goal = goal
        self.repo = repo
        self.loopN = loopN
    }
}

/// Parse `:alias "goal" @repo xN` (any subset, any order after the leading
/// alias). Pure and total — never throws.
public func parseComposerInput(_ raw: String) -> ParsedInput {
    var text = raw.trimmingCharacters(in: .whitespacesAndNewlines)
    var alias: String?
    var repo: String?
    var loopN: Int?

    if text.hasPrefix(":") {
        let after = text.dropFirst()
        let token = after.prefix { !$0.isWhitespace }
        if !token.isEmpty {
            alias = String(token)
            text = String(after.dropFirst(token.count)).trimmingCharacters(in: .whitespacesAndNewlines)
        }
    }

    if let m = firstToken(in: text, prefix: "@") {
        repo = m.token
        text = removeRange(text, m.range)
    }

    if let m = firstLoopCount(in: text) {
        loopN = m.value
        text = removeRange(text, m.range)
    }

    var goal = text
    if goal.count >= 2, goal.hasPrefix("\""), goal.hasSuffix("\"") {
        goal = String(goal.dropFirst().dropLast())
    }
    goal = goal.trimmingCharacters(in: .whitespacesAndNewlines)

    return ParsedInput(alias: alias, goal: goal, repo: repo, loopN: loopN)
}

private func removeRange(_ text: String, _ range: Range<String.Index>) -> String {
    var s = text
    s.removeSubrange(range)
    return s.trimmingCharacters(in: .whitespacesAndNewlines)
}

/// First `@token` (non-whitespace run after the prefix), with its full range.
private func firstToken(in text: String, prefix: Character) -> (token: String, range: Range<String.Index>)? {
    guard let start = text.firstIndex(of: prefix) else { return nil }
    let after = text.index(after: start)
    var end = after
    while end < text.endIndex, !text[end].isWhitespace { end = text.index(after: end) }
    let token = String(text[after..<end])
    if token.isEmpty { return nil }
    return (token, start..<end)
}

/// First `xN` loop-count token (`\bx(\d+)\b`, case-insensitive) with its range.
private func firstLoopCount(in text: String) -> (value: Int, range: Range<String.Index>)? {
    var i = text.startIndex
    while i < text.endIndex {
        let c = text[i]
        if c == "x" || c == "X" {
            let prevOK = i == text.startIndex || !isWordChar(text[text.index(before: i)])
            var j = text.index(after: i)
            let digitsStart = j
            while j < text.endIndex, text[j].isNumber { j = text.index(after: j) }
            if j > digitsStart {
                let nextOK = j == text.endIndex || !isWordChar(text[j])
                if prevOK && nextOK, let value = Int(text[digitsStart..<j]) {
                    return (value, i..<j)
                }
            }
        }
        i = text.index(after: i)
    }
    return nil
}

private func isWordChar(_ c: Character) -> Bool {
    c.isLetter || c.isNumber || c == "_"
}

// MARK: - Card factory

public func makeId() -> String { UUID().uuidString }

/// Build a `StackCard` from raw composer text, optionally forcing a preset.
///
/// `repoOptions` resolves a parsed `@token`'s label (e.g. `"konjoai/lopi"`) to
/// the real absolute path via `resolveRepoToken` before it lands on
/// `config.repo` — `CreateTaskRequest.repo` reaches `git2::Repository::open`
/// with no server-side resolution, so a label stored here would fail to
/// launch. Defaults to `[]` (no resolution, label stored as-is) for callers
/// with no live catalog to resolve against (`makeDraft`, tests); live composer
/// commits always pass the fetched catalog — see `finalizeDraft`.
public func buildCard(_ raw: String, explicitPreset: PresetKey? = nil, repoOptions: [StackOption] = []) -> StackCard {
    let parsed = parseComposerInput(raw)
    let aliasPreset = parsed.alias.flatMap { resolvePresetAlias($0) }
    let presetKey = explicitPreset ?? aliasPreset
    let preset = presetKey.flatMap { PRESET_CATALOG[$0] }
    let resolvedRepo = parsed.repo.map { resolveRepoToken($0, repoOptions) }

    return StackCard(
        id: makeId(),
        preset: presetKey,
        goal: parsed.goal,
        alias: parsed.alias ?? preset?.key.rawValue,
        literal: parsed.alias == nil && presetKey == nil,
        evals: preset?.evals ?? [BASELINE_EVAL],
        status: .idle,
        maxIterations: parsed.loopN ?? DEFAULT_MAX_ITERATIONS,
        iteration: nil,
        scheduled: false,
        cron: defaultCron(),
        guardrails: defaultGuardrails(),
        config: resolvedRepo.map { CardConfig(repo: $0) } ?? CardConfig(),
        taskId: nil
    )
}

// MARK: - Draft card (Creation-Flow-1)

/// A fresh draft card — the pre-commit composer replacement pinned to the top of
/// every pane. Same shape as any card but `status == .draft`, so it renders
/// through the one `StackCardView` with a draft branch rather than a forked
/// `DraftCardView`. Never enters `pane.cards`. Mirrors the web `makeDraft`.
public func makeDraft() -> StackCard {
    var d = buildCard("")
    d.status = .draft
    return d
}

/// True once a draft carries enough to commit: an alias, a non-empty goal, or a
/// template origin. Drives the draft's `hot` (teal) state and the `+ add`
/// button's enabled state. Mirrors the web `draftIsHot`.
public func draftIsHot(_ draft: StackCard) -> Bool {
    draft.alias != nil
        || !draft.goal.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        || draft.tpl != nil
}

/// Commit a draft into a real card. A draft configured via the templates menu
/// (preset or template applied) commits as-is; a still-raw draft honors the
/// inline `:alias @repo ×N` tokens typed into its goal field — the power-user
/// path the retired composer supported. Only ever flips `status` to `.idle`;
/// never mutates the pane. `repoOptions` resolves any inline `@token` label to
/// its real path — see `buildCard`'s doc comment; pass the live catalog
/// whenever one is available (`StackStore.commitDraft` always does). Mirrors
/// the web `finalizeDraft`.
public func finalizeDraft(_ draft: StackCard, repoOptions: [StackOption] = []) -> StackCard {
    if draft.preset != nil || draft.tpl != nil {
        var c = draft
        c.status = .idle
        return c
    }
    let parsed = parseComposerInput(draft.goal)
    if parsed.alias == nil, parsed.repo == nil, parsed.loopN == nil {
        var c = draft
        c.status = .idle
        c.goal = parsed.goal
        c.literal = true
        return c
    }
    var built = buildCard(draft.goal, repoOptions: repoOptions)
    built.id = draft.id
    built.status = .idle
    built.scheduled = draft.scheduled
    built.cron = draft.cron
    built.guardrails = draft.guardrails
    // Web: `config: { ...built.config, ...draft.config }` — the draft's own
    // (drawer-set) overrides win per-field; inline @repo survives otherwise.
    var merged = built.config
    if let v = draft.config.model { merged.model = v }
    if let v = draft.config.effort { merged.effort = v }
    if let v = draft.config.repo { merged.repo = v }
    if let v = draft.config.branch { merged.branch = v }
    if let v = draft.config.autonomy { merged.autonomy = v }
    built.config = merged
    return built
}

/// Whether committing a card should seed the pane's own stack-level repo
/// default — only the first time, while the default is still the cold-start
/// `""` ("auto") sentinel. A later card with a different `@repo` never
/// clobbers an explicit choice (own or user-picked). Pulled out of
/// `StackStore.commitDraft` so the rule is unit-testable without a live store.
public func adoptRepoDefaultIfUnset(_ defaults: StackDefaults, _ committed: StackCard) -> StackDefaults {
    guard defaults.repo.isEmpty, let repo = committed.config.repo, !repo.isEmpty else { return defaults }
    var next = defaults
    next.repo = repo
    return next
}

// MARK: - Pure array ops (unit-tested directly)

/// Prepend a card to the top of the stack.
public func addCard(_ cards: [StackCard], _ card: StackCard) -> [StackCard] {
    [card] + cards
}

/// Drop a card by id. No-op if the id isn't present.
public func removeCard(_ cards: [StackCard], _ id: String) -> [StackCard] {
    cards.filter { $0.id != id }
}

/// Clone a card in place, immediately after the original. Resets run state on
/// the clone. No-op if the id isn't present.
public func duplicateCard(_ cards: [StackCard], _ id: String) -> [StackCard] {
    guard let idx = cards.firstIndex(where: { $0.id == id }) else { return cards }
    var clone = cards[idx]
    clone.id = makeId()
    clone.status = .idle
    clone.iteration = nil
    clone.taskId = nil
    var next = cards
    next.insert(clone, at: idx + 1)
    return next
}

/// Move the card at `from` to index `to` (post-removal indexing). Out-of-range
/// indices are a no-op.
public func reorderCard(_ cards: [StackCard], _ from: Int, _ to: Int) -> [StackCard] {
    guard from >= 0, from < cards.count, to >= 0, to < cards.count else { return cards }
    var next = cards
    let moved = next.remove(at: from)
    next.insert(moved, at: to)
    return next
}

/// Drag-and-drop-friendly reorder: move `fromIndex` to just before/after
/// `targetIndex` (both original-array indices). No-op onto itself.
public func moveCardBeforeOrAfter(_ cards: [StackCard], _ fromIndex: Int, _ targetIndex: Int, _ before: Bool) -> [StackCard] {
    if fromIndex == targetIndex { return cards }
    let to = fromIndex < targetIndex
        ? (before ? targetIndex - 1 : targetIndex)
        : (before ? targetIndex : targetIndex + 1)
    return reorderCard(cards, fromIndex, to)
}

/// Insert a card at a specific index, clamped into range.
public func insertCardAt(_ cards: [StackCard], _ index: Int, _ card: StackCard) -> [StackCard] {
    var next = cards
    let clamped = max(0, min(index, next.count))
    next.insert(card, at: clamped)
    return next
}

/// Patch a single card by id (whole-field replacement via a mutating closure —
/// the Swift analogue of web's shallow-merge `Partial<StackCard>`). No-op if the
/// id isn't present.
public func patchCard(_ cards: [StackCard], _ id: String, _ mutate: (inout StackCard) -> Void) -> [StackCard] {
    guard let idx = cards.firstIndex(where: { $0.id == id }) else { return cards }
    var next = cards
    mutate(&next[idx])
    return next
}

// MARK: - Eval-set ops

/// Toggle one named eval in a card's on-set. The baseline never toggles off.
public func toggleEval(_ evals: [EvalRef], _ name: String) -> [EvalRef] {
    if name == BASELINE_EVAL.name { return evals }
    if evals.contains(where: { $0.name == name }) {
        return evals.filter { $0.name != name }
    }
    guard let found = EVAL_CATALOG.first(where: { $0.name == name }) else { return evals }
    return evals + [found]
}

/// Turn on every eval named in a suite shortcut; already-on evals are untouched.
public func applySuite(_ evals: [EvalRef], _ suiteNames: [String]) -> [EvalRef] {
    let missing = suiteNames
        .filter { name in !evals.contains(where: { $0.name == name }) }
        .compactMap { name in EVAL_CATALOG.first(where: { $0.name == name }) }
    return missing.isEmpty ? evals : evals + missing
}

// MARK: - Iteration stepper

/// Step the *stack* loop-count by `delta`. Three states: `1` = off (run the
/// chain once, no repeat), a literal count `2..N` (no ceiling), and the
/// infinite sentinel `0` (run until the goal/guardrails stop it). Cycles
/// `1 (off) → 2 → ... → N → 0 (∞) → 1`.
public func stepMaxIterations(_ current: Int, _ delta: Int) -> Int {
    if current == 0 { return delta > 0 ? 1 : 0 }
    if current == 1 { return delta > 0 ? MAX_ITERATIONS_FLOOR : 0 }
    let next = current + delta
    return next < MAX_ITERATIONS_FLOOR ? 1 : next
}

/// Display text for the *stack* loop-count pill: `∞` for the infinite
/// sentinel, `off` for a single run with no chain repeat, the plain number
/// otherwise.
public func maxIterationsLabel(_ maxIterations: Int) -> String {
    if maxIterations == 0 { return "∞" }
    if maxIterations == 1 { return "off" }
    return String(maxIterations)
}

/// Step a *card's* `maxIterations` by `delta`. Unlike the stack pill, the card
/// floors at `0` = "off" (single run) and never wraps to the infinite sentinel.
public func stepCardIterations(_ current: Int, _ delta: Int) -> Int {
    max(0, current + delta)
}

/// Display text for a *card's* iteration pill — `off` when disabled (`0`),
/// the plain number otherwise.
public func cardIterationsLabel(_ maxIterations: Int) -> String {
    maxIterations == 0 ? "off" : String(maxIterations)
}

// MARK: - Active-state predicates (drive cardbar highlighting)

public func guardActive(_ g: Guardrails) -> Bool { g.gate || g.until }

public func evalActive(_ card: StackCard) -> Bool { card.evals.count > 1 }

public func configActive(_ card: StackCard, _ defaults: StackDefaults) -> Bool {
    let c = card.config
    return (c.model ?? defaults.model) != defaults.model
        || (c.effort ?? defaults.effort) != defaults.effort
        || (c.repo ?? defaults.repo) != defaults.repo
        || (c.branch ?? defaults.branch) != defaults.branch
        || (c.autonomy ?? defaults.autonomy) != defaults.autonomy
}
