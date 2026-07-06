import SwiftUI

/// The macOS mirror of the web transcript model (`stores/transcript.ts`): the
/// ordered list of rich blocks the chat pane renders. Built here from the live
/// `LiveAgent.logTail` (assistant text, `💭` thinking and `🔧` tool lines, which
/// the Rust spine emits verbatim) plus a few status chips derived from the
/// agent's structured fields.
///
/// NOTE: written to mirror the verified web implementation; this macOS target was
/// not compiled in the authoring environment (Linux) — build on the M3. macOS
/// tool-call blocks surface the call + args; the paired `tool_result` is not on
/// the log tail, so the result body is shown best-effort (flagged in the report).
enum ChipTier { case info, good, warn, bad }

enum TranscriptBlock: Identifiable, Hashable {
    case assistantText(id: String, text: String, streaming: Bool)
    case thinking(id: String, text: String)
    case toolCall(id: String, tool: String, args: String)
    case status(id: String, tier: ChipTier, label: String)

    var id: String {
        switch self {
        case let .assistantText(id, _, _): return id
        case let .thinking(id, _): return id
        case let .toolCall(id, _, _): return id
        case let .status(id, _, _): return id
        }
    }
}

enum TranscriptBuilder {
    /// Build the ordered block list for one agent. Pure: depends only on the
    /// agent's `logTail` + a small set of structured fields.
    static func build(from agent: LiveAgent) -> [TranscriptBlock] {
        var blocks: [TranscriptBlock] = []
        for (i, log) in agent.logTail.enumerated() {
            blocks = fold(blocks, line: log.text, level: log.level, id: "b\(i)", streaming: agent.active)
        }
        // Seal the trailing text block unless the agent is still streaming.
        if !agent.active { blocks = sealText(blocks) }
        blocks.append(contentsOf: trailingChips(agent))
        return blocks
    }

    // MARK: line → block

    private static func fold(_ blocks: [TranscriptBlock], line: String, level: String,
                             id: String, streaming: Bool) -> [TranscriptBlock] {
        let t = line.trimmingCharacters(in: .whitespacesAndNewlines)
        if t.isEmpty { return blocks }
        if t.hasPrefix("🔧") { return appendTool(blocks, label: t, id: id) }
        if t.hasPrefix("💭") { return appendThinking(blocks, strip(t, "💭"), id: id) }
        if t.hasPrefix("●") { return push(blocks, .info, strip(t, "●"), id: id) }
        if t.hasPrefix("⛔") { return push(blocks, .bad, strip(t, "⛔"), id: id) }
        if level == "error" { return push(blocks, .bad, t, id: id) }
        return appendText(blocks, t, id: id, streaming: streaming)
    }

    private static func strip(_ s: String, _ glyph: String) -> String {
        String(s.dropFirst(glyph.count)).trimmingCharacters(in: .whitespaces)
    }

    private static func appendText(_ blocks: [TranscriptBlock], _ line: String,
                                   id: String, streaming: Bool) -> [TranscriptBlock] {
        var out = blocks
        if case let .assistantText(bid, text, true) = out.last {
            out[out.count - 1] = .assistantText(id: bid, text: text + "\n" + line, streaming: true)
            return out
        }
        out.append(.assistantText(id: id, text: line, streaming: true))
        return out
    }

    private static func appendThinking(_ blocks: [TranscriptBlock], _ line: String, id: String) -> [TranscriptBlock] {
        var out = sealText(blocks)
        if case let .thinking(bid, text) = out.last {
            out[out.count - 1] = .thinking(id: bid, text: text + "\n" + line)
            return out
        }
        out.append(.thinking(id: id, text: line))
        return out
    }

    private static func appendTool(_ blocks: [TranscriptBlock], label: String, id: String) -> [TranscriptBlock] {
        // Parse "🔧 Tool(args)" → tool + args.
        let body = strip(label, "🔧")
        let tool: String, args: String
        if let open = body.firstIndex(of: "("), body.hasSuffix(")") {
            tool = String(body[..<open])
            args = String(body[body.index(after: open)..<body.index(before: body.endIndex)])
        } else {
            tool = body
            args = ""
        }
        return sealText(blocks) + [.toolCall(id: id, tool: tool, args: args)]
    }

    private static func push(_ blocks: [TranscriptBlock], _ tier: ChipTier, _ label: String, id: String) -> [TranscriptBlock] {
        sealText(blocks) + [.status(id: id, tier: tier, label: label)]
    }

    private static func sealText(_ blocks: [TranscriptBlock]) -> [TranscriptBlock] {
        var out = blocks
        if case let .assistantText(bid, text, true) = out.last {
            out[out.count - 1] = .assistantText(id: bid, text: text, streaming: false)
        }
        return out
    }

    // MARK: trailing status chips from structured fields

    private static func trailingChips(_ agent: LiveAgent) -> [TranscriptBlock] {
        var chips: [TranscriptBlock] = []
        if let pass = agent.testPassRate {
            chips.append(.status(id: "score", tier: pass >= 0.8 ? .good : .warn,
                                 label: "scored \(Int(pass * 100))% · \(agent.lintErrors ?? 0) lint"))
        }
        if agent.costUsd > 0 {
            chips.append(.status(id: "cost", tier: .info,
                                 label: String(format: "$%.4f · %d turns", agent.costUsd, agent.numTurns)))
        }
        let p = agent.phase.lowercased()
        if p.contains("success") || p == "completed" {
            chips.append(.status(id: "done", tier: .good, label: "completed"))
        } else if p.contains("failed") {
            chips.append(.status(id: "fail", tier: .bad, label: "failed — retries exhausted"))
        }
        return chips
    }
}
