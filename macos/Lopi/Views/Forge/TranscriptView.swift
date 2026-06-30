import SwiftUI

/// The chat body for a macOS agent pane — the mirror of the web `Transcript`. It
/// renders the ordered block list (assistant markdown, collapsible thinking,
/// tool-call accordions, status chips) and keeps the view pinned to the tail
/// while the agent streams. Markdown + fenced code (incl. red/green diffs) are
/// rendered by `MarkdownLogView`; inline emphasis via AttributedString markdown.
///
/// NOTE: written to mirror the verified web implementation; this macOS target was
/// not compiled in the authoring environment (Linux) — build on the M3.
///
/// Text-wrap caveat: SwiftUI `Text` cannot float-wrap around the corner orb, so
/// the bottom-right reserves an L-shaped inset (`orbInset`) rather than a true
/// circular `shape-outside`. This is the flagged fallback from the spec; a
/// TextKit `exclusionPaths` pass over an `NSTextView` is the follow-up.
struct TranscriptView: View {
    var blocks: [TranscriptBlock]
    var streaming: Bool
    /// Reserved bottom-right inset (pt) so text never collides with the orb.
    var orbInset: CGFloat = 0

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 8) {
                    if blocks.isEmpty {
                        Text("— waiting for output —")
                            .font(.system(size: 11, design: .monospaced))
                            .italic()
                            .foregroundStyle(Konjo.fgMute)
                    }
                    ForEach(blocks) { block in
                        row(for: block).id(block.id)
                    }
                    // Reserved corner inset so the floating orb never overlaps text.
                    if orbInset > 0 {
                        Color.clear.frame(height: orbInset * 0.6)
                    }
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 14)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .onChange(of: blocks.count) { _, count in
                guard count > 0, streaming else { return }
                withAnimation(.easeOut(duration: 0.15)) {
                    proxy.scrollTo(blocks.last?.id, anchor: .bottom)
                }
            }
        }
    }

    @ViewBuilder
    private func row(for block: TranscriptBlock) -> some View {
        switch block {
        case let .assistantText(_, text, isStreaming):
            HStack(alignment: .bottom, spacing: 2) {
                MarkdownLogView(text: text, textColor: Konjo.fgDim, baseSize: 12)
                if isStreaming && streaming { Caret() }
            }
        case let .thinking(_, text):
            ThinkingBlock(text: text)
        case let .toolCall(_, tool, args):
            ToolCallBlock(tool: tool, args: args)
        case let .status(_, tier, label):
            StatusChip(tier: tier, label: label)
        }
    }
}

/// A blinking tail caret shown after the open streaming text block.
private struct Caret: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    var body: some View {
        TimelineView(.animation(minimumInterval: 0.5, paused: reduceMotion)) { tl in
            let on = Int(tl.date.timeIntervalSinceReferenceDate * 2) % 2 == 0
            Rectangle()
                .fill(Konjo.ice)
                .frame(width: 7, height: 15)
                .opacity(reduceMotion ? 0.75 : (on ? 0.75 : 0))
        }
    }
}

/// Dim, collapsible reasoning block — off by default, mirroring the web.
private struct ThinkingBlock: View {
    let text: String
    @State private var open = false
    var body: some View {
        DisclosureGroup(isExpanded: $open) {
            Text(text)
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(Konjo.fgMute)
                .padding(.leading, 6)
                .frame(maxWidth: .infinity, alignment: .leading)
        } label: {
            Text("thinking")
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(Konjo.fgMute)
        }
        .tint(Konjo.fgMute)
    }
}

/// A Claude-Code-style tool invocation: a summary line that expands to its args.
/// (The paired result is not on the macOS log tail; see the file note.)
private struct ToolCallBlock: View {
    let tool: String
    let args: String
    @State private var open = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            DisclosureGroup(isExpanded: $open) {
                if !args.isEmpty {
                    Text(args)
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundStyle(Konjo.fgDim)
                        .textSelection(.enabled)
                        .padding(8)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            } label: {
                HStack(spacing: 6) {
                    Text(glyph).foregroundStyle(Konjo.fgDim)
                    Text(tool).font(.system(size: 11, weight: .semibold, design: .monospaced))
                        .foregroundStyle(Konjo.ice)
                    if !args.isEmpty {
                        Text(args).font(.system(size: 11, design: .monospaced))
                            .foregroundStyle(Konjo.fgMute).lineLimit(1)
                    }
                }
            }
            .tint(Konjo.fgMute)
        }
        .padding(8)
        .background(Color.white.opacity(0.02))
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .overlay(RoundedRectangle(cornerRadius: 8).stroke(Konjo.line, lineWidth: 1))
    }

    private var glyph: String {
        switch tool {
        case "Bash": return "$"
        case "Read": return "◰"
        case "Write", "Edit": return "✎"
        case "Glob", "Grep", "WebSearch": return "⌕"
        default: return "🔧"
        }
    }
}

/// A compact inline status chip — phase, score, cost, verdict, terminal.
private struct StatusChip: View {
    let tier: ChipTier
    let label: String
    var body: some View {
        HStack(spacing: 5) {
            Circle().fill(color).frame(width: 5, height: 5)
            Text(label).font(.system(size: 10, design: .monospaced)).foregroundStyle(color)
        }
        .padding(.horizontal, 8).padding(.vertical, 3)
        .background(Capsule().fill(Color.white.opacity(0.03)))
        .overlay(Capsule().stroke(color.opacity(0.4), lineWidth: 1))
    }
    private var color: Color {
        switch tier {
        case .info: return Konjo.fgDim
        case .good: return Konjo.jade
        case .warn: return Konjo.flame
        case .bad: return Konjo.rose
        }
    }
}
