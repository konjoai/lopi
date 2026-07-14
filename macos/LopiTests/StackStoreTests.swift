import XCTest
@testable import Lopi

/// Pure-ops + composer-grammar tests — the Swift port of
/// `web/src/lib/stores/stack.test.ts`, same fixtures and assertions. No SwiftUI,
/// no store, no backend. The acceptance bar for `StackOps`/`StackCron`/
/// `StackPayload`/`StackPaneOps`/`StackSummaries`.
final class StackStoreTests: XCTestCase {

    private func card(_ id: String, _ goal: String? = nil) -> StackCard {
        var c = buildCard("\"\(goal ?? id)\"")
        c.id = id
        return c
    }
    private func pane(_ key: String, _ cards: [StackCard] = [], _ config: StackConfig? = nil) -> StackPaneState {
        StackPaneState(key: key, title: key, cards: cards, config: config ?? defaultStackConfig())
    }
    private let defaults = PaneDefaults(model: "sonnet", effort: "medium", repo: "konjoai/lopi")

    // MARK: add / remove / duplicate / reorder / insert / patch

    func testAddPrepends() {
        XCTAssertEqual(addCard([], card("a")).map(\.id), ["a"], "add into empty stack")
        XCTAssertEqual(addCard([card("a")], card("b")).map(\.id), ["b", "a"], "add prepends to top")
    }

    func testRemove() {
        XCTAssertEqual(removeCard([card("a"), card("b")], "a").map(\.id), ["b"], "remove drops the matching card")
        XCTAssertEqual(removeCard([card("a")], "missing").map(\.id), ["a"], "remove is a no-op for an unknown id")
        XCTAssertEqual(removeCard([card("a")], "a").map(\.id), [], "remove down to empty stack")
    }

    func testDuplicateResetsRunState() {
        var running = card("a")
        running.status = .running
        running.iteration = IterationProgress(current: 1, total: 3)
        running.taskId = "t1"
        let dup = duplicateCard([running, card("b")], "a")
        XCTAssertEqual(dup.count, 3, "duplicate grows the stack by one")
        XCTAssertEqual(dup[0].id, "a", "duplicate keeps the original at its position")
        XCTAssertEqual(dup[1].goal, "a", "duplicate clones the original goal")
        XCTAssertNotEqual(dup[1].id, "a", "duplicate gets a fresh id")
        XCTAssertEqual(dup[1].status, .idle, "duplicate resets status to idle")
        XCTAssertNil(dup[1].iteration, "duplicate clears iteration progress")
        XCTAssertNil(dup[1].taskId, "duplicate clears taskId")
        XCTAssertEqual(dup[2].id, "b", "duplicate does not disturb later cards")
        XCTAssertEqual(duplicateCard([card("a")], "missing").count, 1, "duplicate no-op for unknown id")
    }

    func testReorder() {
        XCTAssertEqual(reorderCard([card("a"), card("b"), card("c")], 0, 2).map(\.id), ["b", "c", "a"], "reorder moves to target")
        XCTAssertEqual(reorderCard([card("a"), card("b")], 0, 99).map(\.id), ["a", "b"], "out-of-range `to` is a no-op")
        XCTAssertEqual(reorderCard([card("a"), card("b")], -1, 1).map(\.id), ["a", "b"], "out-of-range `from` is a no-op")
    }

    func testDragRelativeReorder() {
        let cards = [card("a"), card("b"), card("c"), card("d")]
        XCTAssertEqual(moveCardBeforeOrAfter(cards, 0, 2, true).map(\.id), ["b", "a", "c", "d"], "earlier→just before later")
        XCTAssertEqual(moveCardBeforeOrAfter(cards, 0, 2, false).map(\.id), ["b", "c", "a", "d"], "earlier→just after later")
        XCTAssertEqual(moveCardBeforeOrAfter(cards, 3, 1, true).map(\.id), ["a", "d", "b", "c"], "later→just before earlier")
        XCTAssertEqual(moveCardBeforeOrAfter(cards, 3, 1, false).map(\.id), ["a", "b", "d", "c"], "later→just after earlier")
        XCTAssertEqual(moveCardBeforeOrAfter(cards, 1, 1, true).map(\.id), cards.map(\.id), "onto itself is a no-op")
    }

    func testInsertAndPatch() {
        XCTAssertEqual(insertCardAt([card("a"), card("c")], 1, card("b")).map(\.id), ["a", "b", "c"], "insert at index")
        XCTAssertEqual(insertCardAt([], 5, card("a")).map(\.id), ["a"], "insert clamps out-of-range index")
        let patched = patchCard([card("a"), card("b")], "a") { $0.goal = "renamed" }
        XCTAssertEqual(patched[0].goal, "renamed", "patch merges the given fields")
        XCTAssertEqual(patched[1].goal, "b", "patch leaves other cards untouched")
        XCTAssertEqual(patchCard([card("a")], "missing") { $0.goal = "x" }[0].goal, "a", "patch no-op for unknown id")
    }

    // MARK: composer grammar parser

    func testComposerParser() {
        XCTAssertEqual(parseComposerInput(":optimize \"x\" @squish x3"),
                       ParsedInput(alias: "optimize", goal: "x", repo: "squish", loopN: 3), "alias+goal+repo+loop")
        XCTAssertEqual(parseComposerInput("\"fix the bug\""),
                       ParsedInput(alias: nil, goal: "fix the bug", repo: nil, loopN: nil), "quoted literal, goal-only")
        XCTAssertEqual(parseComposerInput("fix the bug"),
                       ParsedInput(alias: nil, goal: "fix the bug", repo: nil, loopN: nil), "unquoted literal, goal-only")
        XCTAssertEqual(parseComposerInput(":research \"paged attention\""),
                       ParsedInput(alias: "research", goal: "paged attention", repo: nil, loopN: nil), "alias without repo/loop")
    }

    func testKeywordSuggestion() {
        XCTAssertEqual(suggestPreset("add a gate"), .implement, "keyword match suggests implement")
        XCTAssertEqual(suggestPreset("optimize the dequant kernel"), .optimize, "keyword match suggests optimize")
        XCTAssertNil(suggestPreset("draft a changelog entry"), "no keyword match suggests nothing")
    }

    func testBuildCardPresetAttachment() {
        let viaAlias = buildCard(":implement \"add verifier gate\"")
        XCTAssertEqual(viaAlias.preset, .implement, "attaches preset via recognized alias")
        XCTAssertEqual(viaAlias.evals.count, 6, "alias-attached preset carries its full eval suite")
        XCTAssertFalse(viaAlias.literal, "alias-built card is not literal")

        let viaChip = buildCard("improve the dequant kernel", explicitPreset: .optimize)
        XCTAssertEqual(viaChip.preset, .optimize, "attaches preset via explicit chip/grid selection")
        XCTAssertEqual(viaChip.evals.count, 4, "chip-attached preset carries its full eval suite")

        let literal = buildCard("draft weekly changelog digest")
        XCTAssertNil(literal.preset, "no alias/preset ⇒ no preset attached")
        XCTAssertTrue(literal.literal, "plain text builds a literal card")
        XCTAssertEqual(literal.evals, [EvalRef(name: "execution ok", tier: .base)], "literal card carries only baseline")

        let withLoop = buildCard(":optimize \"x\" @squish x3")
        XCTAssertEqual(withLoop.maxIterations, 3, "xN grammar seeds maxIterations")
        XCTAssertEqual(withLoop.config.repo, "squish", "@repo grammar seeds config.repo")

        let plain = buildCard("a plain goal")
        XCTAssertEqual(plain.maxIterations, 0, "no xN ⇒ default off (0) — a fresh card does not loop")
        XCTAssertFalse(plain.scheduled, "fresh card is not scheduled")
        XCTAssertEqual(plain.status, .idle, "fresh card starts idle")
    }

    // MARK: eval-set ops

    func testEvalOps() {
        let toggled = toggleEval([BASELINE_EVAL], "unit")
        XCTAssertEqual(toggled.map(\.name), ["execution ok", "unit"], "toggleEval turns an eval on")
        XCTAssertEqual(toggleEval(toggled, "unit").map(\.name), ["execution ok"], "toggleEval turns it back off")
        XCTAssertEqual(toggleEval([BASELINE_EVAL], "execution ok"), [BASELINE_EVAL], "never turns off the baseline")
        XCTAssertEqual(toggleEval([BASELINE_EVAL], "not-a-real-eval"), [BASELINE_EVAL], "ignores unknown names")

        let suited = applySuite([BASELINE_EVAL], ["vuln scan", "adversarial"])
        XCTAssertEqual(suited.map(\.name), ["execution ok", "vuln scan", "adversarial"], "applySuite adds every named eval")
        XCTAssertEqual(applySuite(suited, ["vuln scan"]).map(\.name), ["execution ok", "vuln scan", "adversarial"], "never duplicates")
    }

    // MARK: iteration stepper

    func testIterationStepper() {
        XCTAssertEqual(stepMaxIterations(25, 1), 26, "stepping up increments")
        XCTAssertEqual(stepMaxIterations(25, -1), 24, "stepping down decrements")
        XCTAssertEqual(stepMaxIterations(2, -1), 0, "below the floor wraps to infinite")
        XCTAssertEqual(stepMaxIterations(3, -2), 0, "multi-step below the floor wraps to infinite")
        XCTAssertEqual(stepMaxIterations(0, 1), 2, "up from infinite lands on the floor, not 1")
        XCTAssertEqual(stepMaxIterations(0, -1), 0, "down from infinite stays infinite")
        XCTAssertEqual(maxIterationsLabel(0), "∞", "label renders infinite sentinel as ∞")
        XCTAssertEqual(maxIterationsLabel(5), "5", "label renders a finite ceiling as its number")

        // Card pill: floors at 0 = "off", never wraps to infinite.
        XCTAssertEqual(stepCardIterations(0, 1), 1, "stepping up from off lands on 1")
        XCTAssertEqual(stepCardIterations(1, -1), 0, "stepping down from 1 reaches off (0)")
        XCTAssertEqual(stepCardIterations(0, -1), 0, "stepping down from off stays off — never wraps to infinite")
        XCTAssertEqual(cardIterationsLabel(0), "off", "card label renders 0 as off")
        XCTAssertEqual(cardIterationsLabel(4), "4", "card label renders a finite ceiling as its number")
        XCTAssertEqual(DEFAULT_MAX_ITERATIONS, 0, "a fresh card defaults to off (0)")
    }

    // MARK: active-state predicates

    func testActivePredicates() {
        XCTAssertFalse(guardActive(defaultGuardrails()), "fresh guardrails are inactive")
        XCTAssertTrue(guardActive({ var g = defaultGuardrails(); g.gate = true; return g }()), "gate alone activates")
        XCTAssertTrue(guardActive({ var g = defaultGuardrails(); g.until = true; return g }()), "until alone activates")
        XCTAssertFalse(evalActive(buildCard("x")), "baseline-only card has inactive evals")
        XCTAssertTrue(evalActive(buildCard(":implement \"x\"")), "preset-attached card has active evals")
        let d = StackDefaults(model: "m", effort: "e", repo: "r", branch: "b", autonomy: "a")
        XCTAssertFalse(configActive(buildCard("x"), d), "no overrides ⇒ config inactive")
        var overridden = buildCard("x"); overridden.config.model = "other"
        XCTAssertTrue(configActive(overridden, d), "a single overridden field activates config")
    }

    // MARK: cron helpers

    func testCronHelpers() {
        XCTAssertEqual(buildCronString(defaultCron()), "0 2 * * *", "default cron (daily 2am)")
        XCTAssertEqual(buildCronString({ var c = defaultCron(); c.freq = .everyMinute; return c }()), "* * * * *", "every-minute")
        XCTAssertEqual(buildCronString({ var c = defaultCron(); c.freq = .hourly; c.min = 15; return c }()), "15 * * * *", "hourly")
        XCTAssertEqual(buildCronString({ var c = defaultCron(); c.freq = .weekly; c.dow = .Fri; c.hour12 = 6; c.ampm = .PM; c.min = 30; return c }()),
                       "30 18 * * 5", "weekly 12h PM + weekday number")
        XCTAssertEqual(buildCronString({ var c = defaultCron(); c.freq = .custom; c.raw = "*/5 * * * *"; return c }()), "*/5 * * * *", "custom passes raw")
        XCTAssertEqual(cronHuman(defaultCron()), "every day at 2:00 AM", "human echo for default cron")
        XCTAssertEqual(cronHuman({ var c = defaultCron(); c.freq = .hourly; c.min = 5; return c }()), "every hour at :05", "human echo pads minutes")
        XCTAssertEqual(cronHuman({ var c = defaultCron(); c.freq = .custom; c.raw = "0 2 * * *"; return c }()), "custom cron",
                       "a custom-flagged cron matching the daily shape still echoes 'custom cron', never snaps")
    }

    func testComputeNextRuns() {
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = TimeZone(identifier: "UTC")!
        func date(_ y: Int, _ mo: Int, _ d: Int, _ h: Int, _ mi: Int) -> Date {
            cal.date(from: DateComponents(year: y, month: mo, day: d, hour: h, minute: mi))!
        }
        let from = date(2026, 7, 8, 10, 0) // a Wednesday
        let daily = computeNextRuns("0 2 * * *", from: from, count: 3, calendar: cal)
        XCTAssertEqual(daily.count, 3, "daily cron finds 3 upcoming runs")
        XCTAssertEqual(cal.component(.hour, from: daily[0]), 2, "each run lands on hour 2")
        XCTAssertEqual(cal.component(.minute, from: daily[0]), 0, "each run lands on minute 0")
        XCTAssertEqual(cal.component(.day, from: daily[0]), 9, "first run after 10am is next day 2am")
        XCTAssertEqual(cal.component(.day, from: daily[1]), 10, "runs are one day apart")

        let everyMin = computeNextRuns("* * * * *", from: from, count: 2, calendar: cal)
        XCTAssertEqual(everyMin.count, 2, "every-minute cron finds runs immediately")
        XCTAssertEqual(everyMin[1].timeIntervalSince(everyMin[0]), 60, accuracy: 0.001, "every-minute runs 60s apart")

        XCTAssertEqual(computeNextRuns("not a cron", from: from, count: 3, calendar: cal), [], "malformed cron yields no results")

        let weekly = computeNextRuns("0 6 * * 5", from: from, count: 1, calendar: cal) // Friday 6am
        XCTAssertEqual(weekly.count, 1, "weekly cron finds the next matching weekday")
        XCTAssertEqual(cal.component(.weekday, from: weekly[0]), 6, "matched run falls on Friday (weekday 6)")
    }

    // MARK: backend round-trip (WIRED fields → payload)

    func testCardPayloadRoundTrip() {
        let plain = buildCard("do the thing")
        let p = cardToTaskPayload(plain, defaults)
        XCTAssertEqual(p.goal, "do the thing", "goal verbatim")
        XCTAssertEqual(p.repo, "konjoai/lopi", "no repo override ⇒ pane default")
        XCTAssertEqual(p.options.model, "sonnet", "no model override ⇒ pane default")
        XCTAssertEqual(p.options.maxIterations, 1, "a fresh (off) card sends a single pass — off (0) maps to max_iterations 1")
        XCTAssertEqual(p.options.onFail, .stop, "default on_fail policy")
        XCTAssertNil(p.options.gate, "gate omitted when toggle off")

        var guarded = buildCard("do the thing")
        guarded.config.repo = "squish"
        guarded.guardrails = Guardrails(gate: true, gateCmd: "./kill_test.sh", until: true, untilCmd: "cargo test", onFail: .backoff, budget: .k200)
        let g = cardToTaskPayload(guarded, defaults)
        XCTAssertEqual(g.options.budgetTokens, 200_000, "'200k' → budget_tokens 200000")
        XCTAssertEqual(g.repo, "squish", "config.repo override wins")
        XCTAssertEqual(g.options.gate, "./kill_test.sh", "enabled gate carries its command")
        XCTAssertEqual(g.options.until, "cargo test", "enabled until carries its command")
        XCTAssertEqual(g.options.onFail, .backoff, "chosen on_fail policy")

        XCTAssertNil(cardToTaskPayload(buildCard("x"), defaults).options.until, "until omitted when toggle off")
    }

    func testBudgetToTokens() {
        XCTAssertEqual(budgetToTokens(.k200), 200_000, "'200k' → 200000-token cap")
        XCTAssertNil(budgetToTokens(.auto), "'auto' inherits — no hard cap")
        XCTAssertNil(budgetToTokens(.none), "'none' uncapped — no hard cap")
        XCTAssertNil(cardToTaskPayload(buildCard("x"), defaults).options.budgetTokens, "inherit preset omits budget_tokens")
    }

    func testLegacyAliasResolves() {
        XCTAssertEqual(resolvePresetAlias("gain"), .gain, "the `:gain` alias resolves to gain")
        XCTAssertEqual(resolvePresetAlias("ratchet"), .gain, "legacy `:ratchet` still resolves to gain")
        XCTAssertNil(resolvePresetAlias("nonsense"), "unknown alias resolves to nil")
        XCTAssertEqual(buildCard(":ratchet \"self improve\"").preset, .gain, "`:ratchet` builds a gain-preset card")
    }

    func testWiredTableRoundTrip() {
        struct Row { let name: String; let apply: (inout StackCard) -> Void; let check: (StackTaskPayload) -> Bool }
        let rows: [Row] = [
            Row(name: "model override", apply: { $0.config.model = "claude-opus-4-8" }, check: { $0.options.model == "claude-opus-4-8" }),
            Row(name: "effort override", apply: { $0.config.effort = "high" }, check: { $0.options.effort == "high" }),
            Row(name: "repo override", apply: { $0.config.repo = "konjoai/squish" }, check: { $0.repo == "konjoai/squish" }),
            Row(name: "gate on", apply: { $0.guardrails.gate = true; $0.guardrails.gateCmd = "./gate.sh" }, check: { $0.options.gate == "./gate.sh" }),
            Row(name: "until on", apply: { $0.guardrails.until = true; $0.guardrails.untilCmd = "exit 0" }, check: { $0.options.until == "exit 0" }),
            Row(name: "on_fail continue", apply: { $0.guardrails.onFail = .continue }, check: { $0.options.onFail == .continue }),
            Row(name: "on_fail backoff", apply: { $0.guardrails.onFail = .backoff }, check: { $0.options.onFail == .backoff }),
            Row(name: "maxIterations 7", apply: { $0.maxIterations = 7 }, check: { $0.options.maxIterations == 7 }),
            Row(name: "maxIterations off (0) → single pass", apply: { $0.maxIterations = 0 }, check: { $0.options.maxIterations == 1 })
        ]
        for row in rows {
            var c = buildCard("table-driven row"); row.apply(&c)
            XCTAssertTrue(row.check(cardToTaskPayload(c, defaults)), "WIRED round-trip: \(row.name)")
        }
        // Field completeness: a fully-guarded, baseline-eval card carries exactly
        // the expected WIRED fields — model/effort/max_iterations/on_fail/
        // client_ref/gate/until/acceptance present; budget_tokens/constraints not.
        var full = buildCard("x")
        full.guardrails = Guardrails(gate: true, gateCmd: "g", until: true, untilCmd: "u", onFail: .stop, budget: .auto)
        let o = cardToTaskPayload(full, defaults).options
        XCTAssertNotNil(o.model); XCTAssertNotNil(o.effort); XCTAssertNotNil(o.maxIterations)
        XCTAssertNotNil(o.onFail); XCTAssertNotNil(o.gate); XCTAssertNotNil(o.until); XCTAssertNotNil(o.acceptance)
        XCTAssertEqual(o.clientRef, full.id, "client_ref always carries the card's own id")
        XCTAssertNil(o.budgetTokens, "no budget_tokens for the inherit preset")
        XCTAssertNil(o.constraints, "no constraints on a card payload")
    }

    func testRunOnceForcesMaxIterations() {
        let c = buildCard(":optimize \"x\" x7")
        XCTAssertEqual(c.maxIterations, 7, "sanity: card carries the xN value")
        XCTAssertEqual(cardToTaskPayloadForRunOnce(c, defaults).options.maxIterations, 1, "Run once forces max_iterations=1")
        XCTAssertEqual(c.maxIterations, 7, "Run once never mutates the card")
        var off = buildCard("x"); off.maxIterations = 0
        XCTAssertEqual(cardToTaskPayloadForRunOnce(off, defaults).options.maxIterations, 1, "Run once on an off (0) card still sends a single pass")
    }

    // MARK: bare pane payload (Unify-1)

    func testPaneSubmitPayload() {
        let bare = paneSubmitPayload(PaneLaunch(goal: "fix foo", repo: ""))
        XCTAssertEqual(bare.goal, "fix foo", "bare prompt carries the goal")
        XCTAssertEqual(bare.repo, "", "bare prompt leaves repo empty")
        XCTAssertEqual(bare.priority, "normal", "bare prompt defaults priority")
        XCTAssertEqual(bare.options, StackTaskOptions(), "a bare prompt sets NO options")

        let p = paneSubmitPayload(PaneLaunch(goal: "g", repo: "konjoai/lopi", priority: "high", model: "claude-opus-4-8", effort: "high"))
        XCTAssertEqual(p.priority, "high", "priority passes through")
        XCTAssertEqual(p.options.model, "claude-opus-4-8", "model is a first-class option")
        XCTAssertEqual(p.options.effort, "high", "effort is a first-class option")
        XCTAssertNil(p.options.constraints, "no branch ⇒ no constraints")
        XCTAssertNil(p.options.maxIterations, "nothing stack-only leaks in")

        let br = paneSubmitPayload(PaneLaunch(goal: "g", repo: "r", branch: "feature/x"))
        XCTAssertEqual(br.options.constraints, ["Target branch: feature/x"], "branch → planning constraint")
        XCTAssertNil(paneSubmitPayload(PaneLaunch(goal: "g", repo: "r", branch: "   ")).options.constraints, "whitespace branch treated as unset")
    }

    func testBarePaneStackParity() {
        struct Row { let name: String; let goal: String; let model: String?; let effort: String?; let priority: String? }
        let rows: [Row] = [
            Row(name: "plain goal", goal: "do the thing", model: nil, effort: nil, priority: nil),
            Row(name: "model+effort", goal: "do the thing", model: "claude-opus-4-8", effort: "high", priority: nil),
            Row(name: "high priority", goal: "urgent", model: nil, effort: nil, priority: "high")
        ]
        for row in rows {
            let paneP = paneSubmitPayload(PaneLaunch(goal: row.goal, repo: defaults.repo, priority: row.priority,
                                                     model: row.model ?? defaults.model, effort: row.effort ?? defaults.effort))
            var c = buildCard("\"\(row.goal)\"")
            if let m = row.model { c.config.model = m }
            if let e = row.effort { c.config.effort = e }
            let stackP = cardToTaskPayload(c, defaults)
            XCTAssertEqual(paneP.goal, stackP.goal, "parity/\(row.name): goal")
            XCTAssertEqual(paneP.repo, stackP.repo, "parity/\(row.name): repo")
            XCTAssertEqual(paneP.options.model, stackP.options.model, "parity/\(row.name): model")
            XCTAssertEqual(paneP.options.effort, stackP.options.effort, "parity/\(row.name): effort")
        }
    }

    // MARK: execution order + dry run

    func testExecutionOrder() {
        let cards = [card("newest"), card("middle"), card("oldest")]
        XCTAssertEqual(executionOrder(cards).map(\.id), ["oldest", "middle", "newest"], "execution order reverses the array")
        XCTAssertEqual(executionOrder([]).map(\.id), [], "execution order of empty stack is empty")
    }

    func testDryRun() {
        let clean = [card("a", "do a"), card("b", "do b")]
        let ok = dryRunStack(clean, defaults)
        XCTAssertTrue(ok.valid, "well-formed cards dry-run clean")
        XCTAssertEqual(ok.issues, [], "no issues on a clean stack")
        XCTAssertEqual(ok.plan.map(\.goal), ["do b", "do a"], "plan is in execution order (oldest first)")

        let empty = buildCard("")
        var badGate = buildCard("has a goal"); badGate.guardrails.gate = true; badGate.guardrails.gateCmd = "   "
        var badUntil = buildCard("also has a goal"); badUntil.guardrails.until = true; badUntil.guardrails.untilCmd = ""
        let bad = dryRunStack([empty, badGate, badUntil], defaults)
        XCTAssertFalse(bad.valid, "any bad card ⇒ invalid overall")
        XCTAssertEqual(bad.issues.count, 3, "each bad card contributes one issue")
        XCTAssertTrue(bad.issues.contains { $0.cardId == empty.id && $0.message.contains("empty") }, "empty-goal flagged by id")
        XCTAssertTrue(bad.issues.contains { $0.cardId == badGate.id && $0.message.contains("gate") }, "empty-gate flagged by id")
        XCTAssertTrue(bad.issues.contains { $0.cardId == badUntil.id && $0.message.contains("until") }, "empty-until flagged by id")
    }

    // MARK: pane-keyed dispatch

    func testPaneKeyedDispatch() {
        let state = [pane("s1", [card("a")]), pane("s2", [card("x")])]
        let inserted = insertIntoPane(state, "s1", 1, card("b"))
        XCTAssertEqual(inserted[0].cards.map(\.id), ["a", "b"], "insertIntoPane inserts into the named pane")
        XCTAssertEqual(inserted[1].cards.map(\.id), ["x"], "leaves the other pane untouched")
        XCTAssertEqual(insertIntoPane(state, "missing", 0, card("b")).map(\.key), state.map(\.key), "unknown key is a no-op")

        let removed = applyToPaneCards([pane("s1", [card("a"), card("b")])], "s1") { $0.filter { $0.id != "a" } }
        XCTAssertEqual(removed[0].cards.map(\.id), ["b"], "applyToPaneCards composes with any card-list op")
    }

    // MARK: bumpInOrder

    func testBumpInOrder() {
        let order = ["a", "b", "c", "d"]
        XCTAssertEqual(bumpInOrder(order, 0, "c", .up), .ok(["a", "c", "b", "d"]), "bump up swaps with predecessor")
        XCTAssertEqual(bumpInOrder(order, 0, "b", .down), .ok(["a", "c", "b", "d"]), "bump down swaps with successor")
        XCTAssertEqual(order, ["a", "b", "c", "d"], "bumpInOrder never mutates the input")
        XCTAssertEqual(bumpInOrder(order, 0, "z", .up), .err("card is not part of this run’s plan"), "absent id rejected")
        XCTAssertEqual(bumpInOrder(order, 1, "a", .down), .err("card is already running or finished — only queued cards can be bumped"), "at/before cursor rejected")
        XCTAssertEqual(bumpInOrder(order, 1, "c", .up), .err("cannot bump above the currently running card"), "landing at/before cursor rejected")
        XCTAssertEqual(bumpInOrder(order, 0, "d", .down), .err("cannot bump past the end of the queue"), "past the end rejected")
    }

    // MARK: stack-level ops

    func testStackLevelOps() {
        let state = [pane("s1", [card("a"), card("b")]), pane("s2", [card("x")])]
        let dup = duplicateStack(state, "s1")
        XCTAssertEqual(dup.map(\.key), [state[0].key, dup[1].key, state[1].key], "clone lands after the original")
        XCTAssertEqual(dup[1].title, "s1 copy", "clone gets a distinguishing title")
        XCTAssertEqual(dup[1].cards.map(\.goal), ["a", "b"], "clone carries every card")
        XCTAssertTrue(zip(dup[1].cards, dup[0].cards).allSatisfy { $0.id != $1.id }, "every cloned card gets a fresh id")
        XCTAssertEqual(duplicateStack([pane("s1"), pane("s2")], "missing").map(\.key), ["s1", "s2"], "unknown key no-op")

        var running = card("a"); running.status = .running; running.iteration = IterationProgress(current: 2, total: 5); running.taskId = "task-123"
        let dupRun = duplicateStack([pane("s1", [running])], "s1")
        XCTAssertEqual(dupRun[1].cards[0].status, .idle, "cloned card resets status")
        XCTAssertNil(dupRun[1].cards[0].taskId, "cloned card drops taskId")
        XCTAssertNil(dupRun[1].cards[0].iteration, "cloned card drops iteration")

        let three = [pane("a"), pane("b"), pane("c")]
        XCTAssertEqual(reorderStacks(three, 0, 2).map(\.key), ["b", "c", "a"], "reorderStacks moves from→to")
        XCTAssertEqual(reorderStacks(three, 0, 9).map(\.key), ["a", "b", "c"], "out-of-range no-op")
        XCTAssertEqual(moveStackBeforeOrAfter(three, 2, 0, true).map(\.key), ["c", "a", "b"], "moveStackBeforeOrAfter(before)")
        XCTAssertEqual(moveStackBeforeOrAfter(three, 0, 2, false).map(\.key), ["b", "c", "a"], "moveStackBeforeOrAfter(after)")
        XCTAssertEqual(deleteStack([pane("s1"), pane("s2")], "s1").map(\.key), ["s2"], "deleteStack drops the named pane")
        XCTAssertEqual(deleteStack([pane("only")], "only").map(\.key), ["only"], "refuses to empty the last pane")
    }

    // MARK: stack-level predicates + B1 goal facet

    func testStackPredicatesAndGoalFacet() {
        let config = defaultStackConfig()
        XCTAssertFalse(stackGuardActive(config.guardrails), "fresh stack guardrails inactive (onFail stop)")
        XCTAssertTrue(stackGuardActive({ var g = config.guardrails; g.onFail = .continue; return g }()), "onFail off stop is active")
        XCTAssertFalse(stackEvalActive(config), "fresh stack evals inactive (baseline only)")
        XCTAssertTrue(stackEvalActive({ var c = config; c.evals = [BASELINE_EVAL, EvalRef(name: "tests pass", tier: .test)]; return c }()), "more than baseline active")
        XCTAssertFalse(stackDefaultsActive(config.defaults), "fresh stack defaults inactive")
        XCTAssertTrue(stackDefaultsActive({ var d = DEFAULT_STACK_DEFAULTS; d.model = "claude-sonnet-4-6"; return d }()), "moved-off default active")

        XCTAssertFalse(config.goal.pursue, "fresh stack does not pursue a goal")
        XCTAssertEqual(defaultStackGoal().noProgressLimit, 3, "default no-progress tolerance is 3")
        XCTAssertFalse(stackGoalActive(config), "fresh goal facet inactive")
        XCTAssertTrue(stackGoalActive({ var c = config; c.goal = StackGoal(pursue: true, noProgressLimit: 3); return c }()), "pursue on is active")
        XCTAssertFalse(stackPursuesGoal({ var c = config; c.goal = StackGoal(pursue: true, noProgressLimit: 3); return c }()), "pursue+baseline-only is inert")
        XCTAssertTrue(stackPursuesGoal({ var c = config; c.evals = [BASELINE_EVAL, EvalRef(name: "tests pass", tier: .test)]; c.goal = StackGoal(pursue: true, noProgressLimit: 3); return c }()), "pursue+real acceptance is a real goal")
        XCTAssertFalse(stackPursuesGoal({ var c = config; c.evals = [BASELINE_EVAL, EvalRef(name: "tests pass", tier: .test)]; return c }()), "acceptance without pursue is not pursued")
        XCTAssertTrue(stackGoalSummary({ var c = config; c.loopCount = 5; return c }()).contains("≤5"), "finite ceiling shows the cap")
        XCTAssertTrue(stackGoalSummary({ var c = config; c.loopCount = 0; return c }()).contains("until met"), "infinite ceiling reads 'until met'")
    }

    func testPerLoopScheduleGoverned() {
        let config = defaultStackConfig()
        XCTAssertFalse(perLoopScheduleGoverned(config), "un-scheduled ×1 stack does not govern")
        XCTAssertTrue(perLoopScheduleGoverned({ var c = config; c.scheduled = true; return c }()), "scheduled stack governs")
        XCTAssertTrue(perLoopScheduleGoverned({ var c = config; c.loopCount = 3; return c }()), "×3 stack governs")
        XCTAssertTrue(perLoopScheduleGoverned({ var c = config; c.loopCount = 0; return c }()), "×∞ stack governs")
        XCTAssertFalse(perLoopScheduleGoverned({ var c = config; c.loopCount = 1; c.scheduled = false; return c }()), "explicitly ×1 unscheduled does not govern")
    }

    func testDefaultResolutionPrecedence() {
        var sd = DEFAULT_STACK_DEFAULTS; sd.model = "claude-sonnet-4-6"; sd.effort = "high"; sd.repo = "konjoai/stack-repo"
        let d = PaneDefaults(sd)
        XCTAssertEqual(cardToTaskPayload(buildCard("row"), d).options.model, "claude-sonnet-4-6", "unset loop inherits stack default (model)")
        var mo = buildCard("row"); mo.config.model = "claude-opus-4-8"
        XCTAssertEqual(cardToTaskPayload(mo, d).options.model, "claude-opus-4-8", "loop override beats stack default (model)")
        XCTAssertEqual(cardToTaskPayload(buildCard("row"), d).options.effort, "high", "unset loop inherits stack default (effort)")
        XCTAssertEqual(cardToTaskPayload(buildCard("row"), d).repo, "konjoai/stack-repo", "unset loop inherits stack default (repo)")
        var ro = buildCard("row"); ro.config.repo = "konjoai/other"
        XCTAssertEqual(cardToTaskPayload(ro, d).repo, "konjoai/other", "loop override beats stack default (repo)")
        XCTAssertEqual(cardToTaskPayload(buildCard("no overrides"), PaneDefaults(DEFAULT_STACK_DEFAULTS)).options.model, DEFAULT_STACK_DEFAULTS.model, "DEF wins through both rungs")
    }

    // MARK: evals → acceptance (A1)

    func testEvalsToAcceptance() {
        let base = evalsToAcceptance([BASELINE_EVAL])
        XCTAssertNotNil(base, "baseline compiles into a real acceptance")
        XCTAssertEqual(base?.checks.count, 1, "baseline alone ⇒ one check")
        XCTAssertEqual(base?.checks[0].spec, .executionOk, "baseline ⇒ deterministic execution_ok")
        XCTAssertEqual(base?.checks[0].required, true, "baseline check is a hard gate")

        let baseTest = evalsToAcceptance([BASELINE_EVAL, EvalRef(name: "tests pass", tier: .test), EvalRef(name: "unit", tier: .test)])
        XCTAssertEqual(baseTest?.checks.map(\.spec.kind), ["execution_ok"], "base + tests ⇒ one deterministic check")

        let judge = evalsToAcceptance([BASELINE_EVAL, EvalRef(name: "code review", tier: .judge), EvalRef(name: "beats-best", tier: .judge)])
        let judgeCheck = judge?.checks.first { $0.spec.kind == "judge" }
        if case let .judge(_, criteria) = judgeCheck?.spec {
            XCTAssertEqual(criteria, ["code review", "beats-best"], "judge rubric carries every judge eval name")
        } else { XCTFail("expected a judge check") }
        XCTAssertEqual(judge?.checks.filter { $0.spec.kind == "judge" }.count, 1, "all judge evals fold into one judge check")

        let suites = evalsToAcceptance([BASELINE_EVAL, EvalRef(name: "vuln scan", tier: .suite), EvalRef(name: "adversarial", tier: .suite)])
        let suiteChecks = suites?.checks.filter { $0.spec.kind == "suite" } ?? []
        XCTAssertEqual(suiteChecks.count, 2, "two suite evals ⇒ two suite checks")
        XCTAssertNil(evalsToAcceptance([]), "no evals ⇒ no acceptance")

        var c = buildCard("ship it"); c.evals = [BASELINE_EVAL, EvalRef(name: "code review", tier: .judge)]
        XCTAssertNotNil(cardToTaskPayload(c, defaults).options.acceptance, "cardToTaskPayload emits a real acceptance")
        XCTAssertEqual(cardToTaskPayload(c, defaults).options.acceptance?.checks.count, 2, "base + judge ⇒ two checks")
    }

    // MARK: bare vs stack chrome + pane creation (Unify-2 §3)

    func testPaneIsBareAndCreation() {
        let cfg = defaultStackConfig()
        XCTAssertTrue(paneIsBare(StackPaneState(key: "e", title: "t", cards: [], config: cfg)), "empty pane is bare")
        XCTAssertTrue(paneIsBare(StackPaneState(key: "o", title: "t", cards: [buildCard("a")], config: cfg)), "single-card pane is bare")
        XCTAssertFalse(paneIsBare(StackPaneState(key: "w", title: "t", cards: [buildCard("a"), buildCard("b")], config: cfg)), "second loop earns the chrome")

        let blank = makeBlankStack()
        XCTAssertEqual(blank.cards.count, 0, "fresh pane starts empty")
        XCTAssertFalse(blank.key.isEmpty, "fresh pane has a unique key")
        XCTAssertNotEqual(blank.key, makeBlankStack().key, "each fresh pane gets its own key")

        let grown = addStack([makeBlankStack("one")])
        XCTAssertEqual(grown.count, 2, "addStack appends one pane")
    }

    // MARK: Creation-Flow-1 — draft card, templates, provenance (mirrors web)

    func testMakeDraftAndHot() {
        let d = makeDraft()
        XCTAssertEqual(d.status, .draft, "makeDraft starts in the draft status")
        XCTAssertEqual(d.goal, "", "a fresh draft has an empty goal")
        XCTAssertFalse(draftIsHot(d), "an empty draft is not hot")
        var withGoal = d; withGoal.goal = "fix foo"
        XCTAssertTrue(draftIsHot(withGoal), "a draft with goal text is hot")
        var withAlias = d; withAlias.alias = "research"
        XCTAssertTrue(draftIsHot(withAlias), "a draft with an alias is hot")
        var withTpl = d; withTpl.tpl = "kcqf sprint"
        XCTAssertTrue(draftIsHot(withTpl), "a draft with a template origin is hot")
    }

    func testDraftExcludedFromRun() {
        let runnable = buildCard("do the thing")
        let order = executionOrder([makeDraft(), runnable])
        XCTAssertEqual(order.count, 1, "a draft is excluded from the execution order")
        XCTAssertEqual(order[0].id, runnable.id, "only the committed card runs")
        XCTAssertTrue(executionOrder([makeDraft()]).isEmpty, "a lone draft yields an empty run plan")
    }

    func testFinalizeDraftFoldsInlineTokens() {
        var draft = makeDraft()
        draft.goal = ":research investigate X @konjoai/lopi x3"
        let c = finalizeDraft(draft)
        XCTAssertEqual(c.status, .idle, "finalizeDraft commits to idle")
        XCTAssertEqual(c.preset, .research, "inline :alias resolves to its preset")
        XCTAssertEqual(c.goal, "investigate X", "tokens are stripped from the committed goal")
        XCTAssertEqual(c.config.repo, "konjoai/lopi", "inline @repo lands on config")
        XCTAssertEqual(c.maxIterations, 3, "inline xN sets the iteration ceiling")
    }

    func testFinalizeDraftKeepsConfiguredDraft() {
        var draft = applyPreset(.implement, to: makeDraft())
        draft.goal = "build the widget"
        let c = finalizeDraft(draft)
        XCTAssertEqual(c.preset, .implement, "a configured draft keeps its preset on commit")
        XCTAssertEqual(c.goal, "build the widget", "a configured draft keeps its literal goal")
    }

    func testApplyPresetClearsProvenance() {
        var withTpl = makeDraft(); withTpl.tpl = "x"; withTpl.tplKind = .prompt
        let p = applyPreset(.optimize, to: withTpl)
        XCTAssertEqual(p.preset, .optimize, "applyPreset sets the preset")
        XCTAssertEqual(p.alias, "optimize", "applyPreset sets the alias to the preset key")
        XCTAssertEqual(p.evals, PRESET_CATALOG[.optimize]?.evals, "applyPreset attaches the preset eval suite")
        XCTAssertNil(p.tpl, "picking a bare preset clears template provenance")
        XCTAssertNil(p.tplKind, "picking a bare preset clears the template kind")
    }

    func testProvenanceSurvivesEdit() {
        let tpl = PromptTemplate(id: "t1", name: "deep research", preset: .research, alias: nil, goal: "investigate")
        let filled = applyPromptTemplate(tpl, to: makeDraft())
        XCTAssertEqual(filled.tpl, "deep research", "applyPromptTemplate stamps the template name")
        XCTAssertEqual(filled.tplKind, .prompt, "prompt-template provenance kind")
        XCTAssertEqual(filled.preset, .research, "the template preset drives evals/config")
        var edited = filled; edited.goal = "investigate something else entirely"
        XCTAssertEqual(edited.tpl, "deep research", "provenance survives an edit to goal")
        XCTAssertEqual(edited.tplKind, .prompt, "provenance kind survives an edit to goal")
        XCTAssertEqual(finalizeDraft(edited).tpl, "deep research", "provenance survives commit")
    }

    func testStackTemplateBottomFirstRoundTrip() {
        // Build a pane the way addCard does (prepend → newest on top, bottom
        // runs first); serialize; apply into an empty pane; assert same run order.
        var cards: [StackCard] = []
        cards = addCard(cards, buildCard(":research first"))    // added first → bottom → runs first
        cards = addCard(cards, buildCard(":implement second"))
        cards = addCard(cards, buildCard(":optimize third"))    // added last → top → runs last
        let runBefore = executionOrder(cards).map(\.goal)
        XCTAssertEqual(runBefore, ["first", "second", "third"], "sanity: bottom card runs first")

        let tpl = stackTemplate(from: cards, name: "my chain")
        XCTAssertEqual(tpl.loops.first?.goal, "first", "serialized bottom-first: first-to-run is loop[0]")
        XCTAssertEqual(tpl.loops.first?.preset, .research, "serialized loop carries its preset")

        let restored = applyStackTemplate(tpl, into: [])
        XCTAssertEqual(executionOrder(restored).map(\.goal), runBefore, "round-trips into the same run order")
        XCTAssertEqual(restored.last?.goal, "first", "template's first loop lands at the bottom")
        XCTAssertEqual(stackTemplate(from: restored, name: "again").loops.map(\.goal),
                       tpl.loops.map(\.goal), "double round-trip is stable")
    }

    func testStackTemplateLoopProvenance() {
        let tpl = StackTemplate(id: "s1", name: "kcqf", loops: [
            TemplateLoop(preset: .research, alias: nil, goal: "r"),
            TemplateLoop(preset: .implement, alias: nil, goal: "i")
        ])
        let cards = applyStackTemplate(tpl, into: [])
        XCTAssertTrue(cards.allSatisfy { $0.tplKind == .stack && $0.tpl == "kcqf" }, "every dropped loop carries stack provenance")
        XCTAssertTrue(cards.allSatisfy { $0.alias != nil }, "each loop keeps its own preset alias")
    }

    func testPromptTemplateFromCard() {
        var c = applyPreset(.benchmark, to: makeDraft())
        c.goal = "measure throughput"
        let t = promptTemplate(from: c, name: "bench it")
        XCTAssertEqual(t.name, "bench it", "prompt template takes the given name")
        XCTAssertEqual(t.preset, .benchmark, "prompt template captures the preset")
        XCTAssertEqual(t.goal, "measure throughput", "prompt template captures the goal")
    }

    @MainActor
    func testCommitDraftFlowAndDraftNeverInCards() {
        let store = StackStore(panes: [pane("p")])
        store.updateDraftInPane("p") { $0 = applyPreset(.research, to: $0); $0.goal = "survey" }
        XCTAssertEqual(store.pane(for: "p")?.draft.status, .draft, "the draft is a draft before commit")
        store.commitDraft("p")
        let p = store.pane(for: "p")
        XCTAssertEqual(p?.cards.count, 1, "commit adds one real card")
        XCTAssertEqual(p?.cards.first?.status, .idle, "the committed card is idle, not draft")
        XCTAssertEqual(p?.cards.first?.preset, .research, "the committed card keeps its preset")
        XCTAssertEqual(p?.draft.status, .draft, "a fresh draft is minted after commit")
        XCTAssertTrue(p?.cards.contains { $0.status == .draft } == false, "no draft ever lands in pane.cards (reorder/dnd never sees it)")
    }
}
