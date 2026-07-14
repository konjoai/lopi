import Foundation

// Templates (presets + prompt/stack templates) — the pure Swift port of the web
// `stores/stack.ts` template block (Creation-Flow-1). Same names, same ordering,
// same semantics; any divergence is a bug, not a platform idiom. Foundation only.

// MARK: - Template types (mirror the web PromptTemplate / StackTemplate)

/// A saved single-loop template: a preset and/or alias plus goal text. Client
/// provenance only (`tpl`/`tplKind` on the produced card) — applying it fills a
/// draft, it does not bind the card to the template afterward.
struct PromptTemplate: Codable, Identifiable, Hashable {
    var id: String
    var name: String
    var preset: PresetKey?
    var alias: String?
    var goal: String
}

/// A saved multi-loop chain template. `loops` is serialized **bottom-first**
/// (execution order — first-to-run first) by `stackTemplate(from:name:)`, so
/// `applyStackTemplate(_:into:)` round-trips it back into the same run order.
struct StackTemplate: Codable, Identifiable, Hashable {
    var id: String
    var name: String
    var loops: [TemplateLoop]
}

/// One rung of a stack template — preset/alias/goal, no per-loop config (loops
/// carry no `@repo`/`×N`, matching the web `StackTemplate.loops` shape).
struct TemplateLoop: Codable, Hashable {
    var preset: PresetKey?
    var alias: String?
    var goal: String
}

// MARK: - Pure apply / serialize functions (mirror the web 1:1)

/// Attach a preset to a card: sets `preset`/`alias`/`evals` from the catalog and
/// clears any template provenance (picking a bare preset is not a template
/// origin). Leaves `goal` and every configured facet untouched. Mirrors
/// the web `applyPreset`.
func applyPreset(_ key: PresetKey, to card: StackCard) -> StackCard {
    guard let p = PRESET_CATALOG[key] else { return card }
    var c = card
    c.preset = key
    c.alias = p.key.rawValue
    c.evals = p.evals
    c.literal = false
    c.tpl = nil
    c.tplKind = nil
    return c
}

/// Fill a card from a prompt template: preset/alias/goal/evals from the catalog,
/// plus prompt provenance (`tpl`/`tplKind == .prompt`). The preset (if any) still
/// drives evals/config exactly as a hand-picked preset would. Mirrors the web
/// `applyPromptTemplate`.
func applyPromptTemplate(_ tpl: PromptTemplate, to card: StackCard) -> StackCard {
    let presetKey = tpl.preset ?? tpl.alias.flatMap { resolvePresetAlias($0) }
    let preset = presetKey.flatMap { PRESET_CATALOG[$0] }
    var c = card
    c.preset = presetKey
    c.alias = tpl.alias ?? preset?.key.rawValue
    c.goal = tpl.goal
    c.evals = preset?.evals ?? [BASELINE_EVAL]
    c.literal = false
    c.tpl = tpl.name
    c.tplKind = .prompt
    return c
}

/// Build one committed card from a stack-template loop, stamped with stack
/// provenance. Mirrors `buildCard`'s preset resolution, from a structured loop.
private func cardFromLoop(_ loop: TemplateLoop, tplName: String) -> StackCard {
    let presetKey = loop.preset ?? loop.alias.flatMap { resolvePresetAlias($0) }
    let preset = presetKey.flatMap { PRESET_CATALOG[$0] }
    return StackCard(
        id: makeId(),
        preset: presetKey,
        goal: loop.goal,
        alias: loop.alias ?? preset?.key.rawValue,
        literal: presetKey == nil && loop.alias == nil,
        evals: preset?.evals ?? [BASELINE_EVAL],
        status: .idle,
        maxIterations: DEFAULT_MAX_ITERATIONS,
        iteration: nil,
        scheduled: false,
        cron: defaultCron(),
        guardrails: defaultGuardrails(),
        config: CardConfig(),
        taskId: nil,
        tpl: tplName,
        tplKind: .stack)
}

/// Drop a whole chain template into a pane's cards. `addCard` prepends (newest
/// on top; the **bottom** card is oldest and runs first), so to land the
/// template's **first loop at the bottom** the loops are prepended in reverse.
/// Round-trips with `stackTemplate(from:name:)`. Mirrors the web
/// `applyStackTemplate`.
func applyStackTemplate(_ tpl: StackTemplate, into cards: [StackCard]) -> [StackCard] {
    let loopCards = tpl.loops.map { cardFromLoop($0, tplName: tpl.name) }
    return loopCards.reversed() + cards
}

/// Serialize a card into a reusable prompt template (provenance is not carried —
/// a template is a fresh origin, not a copy of another template's lineage).
/// Mirrors the web `promptTemplateFromCard`.
func promptTemplate(from card: StackCard, name: String) -> PromptTemplate {
    PromptTemplate(id: makeId(), name: name, preset: card.preset, alias: card.alias, goal: card.goal)
}

/// Serialize a pane's cards into a stack template **bottom-first** (execution
/// order) so `applyStackTemplate(_:into:)` restores the identical run order —
/// the easiest thing to get backwards, hence the explicit round-trip test.
/// Mirrors the web `stackTemplateFromCards`.
func stackTemplate(from cards: [StackCard], name: String) -> StackTemplate {
    StackTemplate(
        id: makeId(),
        name: name,
        loops: executionOrder(cards).map { TemplateLoop(preset: $0.preset, alias: $0.alias, goal: $0.goal) })
}
