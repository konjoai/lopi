import Foundation

// Pure card + eval + iteration + predicate ops — the port of the tested pure
// functions in `stores/stack.ts`. Total and side-effect-free; unit-tested by
// `StackStoreTests` exactly as the web `stack.test.ts` tests them. Foundation
// only, no UI import.

// MARK: - Budget

/// Resolve a budget preset to the enforced per-loop token cap, or `nil` when the
/// preset sets no hard cap (`auto` inherits, `none` is uncapped — both omit the
/// field so the payload never claims a limit the loop won't enforce).
func budgetToTokens(_ budget: Budget) -> Int? {
    budget == .k200 ? 200_000 : nil
}

// MARK: - Preset resolution / suggestion

private func isPresetKey(_ s: String) -> Bool {
    PresetKey(rawValue: s) != nil
}

/// Resolve a raw alias token (without the leading `:`) to a preset key, applying
/// legacy renames. Returns `nil` when it names no known preset.
func resolvePresetAlias(_ alias: String) -> PresetKey? {
    if let key = PresetKey(rawValue: alias) { return key }
    return LEGACY_ALIASES[alias]
}

/// Keyword-match a typed goal against the preset catalog. Highlight-only —
/// callers must never auto-attach the result. Returns the first matching preset,
/// or `nil`.
func suggestPreset(_ text: String) -> PresetKey? {
    let lower = text.lowercased()
    for key in PRESET_KEYS {
        if let def = PRESET_CATALOG[key], def.keywords.contains(where: { lower.contains($0) }) {
            return key
        }
    }
    return nil
}

// MARK: - Composer grammar parser

/// The pieces a composer/CLI string parses into.
struct ParsedInput: Equatable {
    var alias: String?
    var goal: String
    var repo: String?
    var loopN: Int?
}

/// Parse `:alias "goal" @repo xN` (any subset, any order after the leading
/// alias). Pure and total — never throws.
func parseComposerInput(_ raw: String) -> ParsedInput {
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

func makeId() -> String { UUID().uuidString }

/// Build a `StackCard` from raw composer text, optionally forcing a preset.
func buildCard(_ raw: String, explicitPreset: PresetKey? = nil) -> StackCard {
    let parsed = parseComposerInput(raw)
    let aliasPreset = parsed.alias.flatMap { resolvePresetAlias($0) }
    let presetKey = explicitPreset ?? aliasPreset
    let preset = presetKey.flatMap { PRESET_CATALOG[$0] }

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
        config: parsed.repo.map { CardConfig(repo: $0) } ?? CardConfig(),
        taskId: nil
    )
}

// MARK: - Draft card (Creation-Flow-1)

/// A fresh draft card — the pre-commit composer replacement pinned to the top of
/// every pane. Same shape as any card but `status == .draft`, so it renders
/// through the one `StackCardView` with a draft branch rather than a forked
/// `DraftCardView`. Never enters `pane.cards`. Mirrors the web `makeDraft`.
func makeDraft() -> StackCard {
    var d = buildCard("")
    d.status = .draft
    return d
}

/// True once a draft carries enough to commit: an alias, a non-empty goal, or a
/// template origin. Drives the draft's `hot` (teal) state and the `+ add`
/// button's enabled state. Mirrors the web `draftIsHot`.
func draftIsHot(_ draft: StackCard) -> Bool {
    draft.alias != nil
        || !draft.goal.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        || draft.tpl != nil
}

/// Commit a draft into a real card. A draft configured via the templates menu
/// (preset or template applied) commits as-is; a still-raw draft honors the
/// inline `:alias @repo ×N` tokens typed into its goal field — the power-user
/// path the retired composer supported. Only ever flips `status` to `.idle`;
/// never mutates the pane. Mirrors the web `finalizeDraft`.
func finalizeDraft(_ draft: StackCard) -> StackCard {
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
    var built = buildCard(draft.goal)
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

// MARK: - Pure array ops (unit-tested directly)

/// Prepend a card to the top of the stack.
func addCard(_ cards: [StackCard], _ card: StackCard) -> [StackCard] {
    [card] + cards
}

/// Drop a card by id. No-op if the id isn't present.
func removeCard(_ cards: [StackCard], _ id: String) -> [StackCard] {
    cards.filter { $0.id != id }
}

/// Clone a card in place, immediately after the original. Resets run state on
/// the clone. No-op if the id isn't present.
func duplicateCard(_ cards: [StackCard], _ id: String) -> [StackCard] {
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
func reorderCard(_ cards: [StackCard], _ from: Int, _ to: Int) -> [StackCard] {
    guard from >= 0, from < cards.count, to >= 0, to < cards.count else { return cards }
    var next = cards
    let moved = next.remove(at: from)
    next.insert(moved, at: to)
    return next
}

/// Drag-and-drop-friendly reorder: move `fromIndex` to just before/after
/// `targetIndex` (both original-array indices). No-op onto itself.
func moveCardBeforeOrAfter(_ cards: [StackCard], _ fromIndex: Int, _ targetIndex: Int, _ before: Bool) -> [StackCard] {
    if fromIndex == targetIndex { return cards }
    let to = fromIndex < targetIndex
        ? (before ? targetIndex - 1 : targetIndex)
        : (before ? targetIndex : targetIndex + 1)
    return reorderCard(cards, fromIndex, to)
}

/// Insert a card at a specific index, clamped into range.
func insertCardAt(_ cards: [StackCard], _ index: Int, _ card: StackCard) -> [StackCard] {
    var next = cards
    let clamped = max(0, min(index, next.count))
    next.insert(card, at: clamped)
    return next
}

/// Patch a single card by id (whole-field replacement via a mutating closure —
/// the Swift analogue of web's shallow-merge `Partial<StackCard>`). No-op if the
/// id isn't present.
func patchCard(_ cards: [StackCard], _ id: String, _ mutate: (inout StackCard) -> Void) -> [StackCard] {
    guard let idx = cards.firstIndex(where: { $0.id == id }) else { return cards }
    var next = cards
    mutate(&next[idx])
    return next
}

// MARK: - Eval-set ops

/// Toggle one named eval in a card's on-set. The baseline never toggles off.
func toggleEval(_ evals: [EvalRef], _ name: String) -> [EvalRef] {
    if name == BASELINE_EVAL.name { return evals }
    if evals.contains(where: { $0.name == name }) {
        return evals.filter { $0.name != name }
    }
    guard let found = EVAL_CATALOG.first(where: { $0.name == name }) else { return evals }
    return evals + [found]
}

/// Turn on every eval named in a suite shortcut; already-on evals are untouched.
func applySuite(_ evals: [EvalRef], _ suiteNames: [String]) -> [EvalRef] {
    let missing = suiteNames
        .filter { name in !evals.contains(where: { $0.name == name }) }
        .compactMap { name in EVAL_CATALOG.first(where: { $0.name == name }) }
    return missing.isEmpty ? evals : evals + missing
}

// MARK: - Iteration stepper

/// Step the *stack* loop-count by `delta`. Floors at `MAX_ITERATIONS_FLOOR`;
/// below it wraps to the infinite sentinel (`0`), so a goal-pursuing chain can
/// still be set to run "until met". Up from infinite skips to the floor.
func stepMaxIterations(_ current: Int, _ delta: Int) -> Int {
    if current == 0 { return delta > 0 ? MAX_ITERATIONS_FLOOR : 0 }
    let next = current + delta
    return next < MAX_ITERATIONS_FLOOR ? 0 : next
}

/// Display text for the *stack* loop-count pill (`∞` for the infinite sentinel).
func maxIterationsLabel(_ maxIterations: Int) -> String {
    maxIterations == 0 ? "∞" : String(maxIterations)
}

/// Step a *card's* `maxIterations` by `delta`. Unlike the stack pill, the card
/// floors at `0` = "off" (single run) and never wraps to the infinite sentinel.
func stepCardIterations(_ current: Int, _ delta: Int) -> Int {
    max(0, current + delta)
}

/// Display text for a *card's* iteration pill — `off` when disabled (`0`),
/// the plain number otherwise.
func cardIterationsLabel(_ maxIterations: Int) -> String {
    maxIterations == 0 ? "off" : String(maxIterations)
}

// MARK: - Active-state predicates (drive cardbar highlighting)

func guardActive(_ g: Guardrails) -> Bool { g.gate || g.until }

func evalActive(_ card: StackCard) -> Bool { card.evals.count > 1 }

func configActive(_ card: StackCard, _ defaults: StackDefaults) -> Bool {
    let c = card.config
    return (c.model ?? defaults.model) != defaults.model
        || (c.effort ?? defaults.effort) != defaults.effort
        || (c.repo ?? defaults.repo) != defaults.repo
        || (c.branch ?? defaults.branch) != defaults.branch
        || (c.autonomy ?? defaults.autonomy) != defaults.autonomy
}
