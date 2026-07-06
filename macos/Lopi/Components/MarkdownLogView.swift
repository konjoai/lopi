import SwiftUI

/// Renders a Claude log line as stacked, individually-formatted markdown blocks:
/// fenced ```code``` becomes a monospace box, `#` headings / `-`,`*` bullets /
/// plain paragraphs each render in their own view. Inline emphasis (`**bold**`,
/// `*italic*`, `` `code` ``) is applied within text blocks. Falls back to plain
/// text when a segment can't be parsed.
struct MarkdownLogView: View {
    let text: String
    let textColor: Color
    var baseSize: CGFloat = 9

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            ForEach(Array(MarkdownLogBlock.parse(text).enumerated()), id: \.offset) { _, block in
                view(for: block)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    @ViewBuilder
    private func view(for block: MarkdownLogBlock) -> some View {
        switch block {
        case let .code(code, lang):
            CodeBlockView(code: code, lang: lang, baseSize: baseSize)
        case let .heading(title, level):
            Text(inline(title))
                .font(.system(size: baseSize + (level <= 1 ? 4 : 2),
                              weight: .bold, design: .monospaced))
                .foregroundStyle(Konjo.fg)
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
        case let .bullets(items):
            VStack(alignment: .leading, spacing: 2) {
                ForEach(Array(items.enumerated()), id: \.offset) { _, item in
                    HStack(alignment: .top, spacing: 6) {
                        Text("•").foregroundStyle(Konjo.fgMute)
                        Text(inline(item)).foregroundStyle(textColor)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    .font(.system(size: baseSize, design: .monospaced))
                    .textSelection(.enabled)
                }
            }
        case let .paragraph(body):
            Text(inline(body))
                .font(.system(size: baseSize, design: .monospaced))
                .foregroundStyle(textColor)
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    /// Parse a single segment's inline markdown, preserving its whitespace.
    private func inline(_ md: String) -> AttributedString {
        (try? AttributedString(
            markdown: md,
            options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace)
        )) ?? AttributedString(md)
    }
}

/// A fenced code block in its own bordered, horizontally-scrollable box, with an
/// optional language tag.
private struct CodeBlockView: View {
    let code: String
    let lang: String?
    var baseSize: CGFloat

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if let lang, !lang.isEmpty {
                Text(lang.uppercased())
                    .font(.system(size: max(baseSize - 2, 6), weight: .semibold, design: .monospaced))
                    .tracking(0.8)
                    .foregroundStyle(Konjo.fgMute)
                    .padding(.horizontal, 8).padding(.top, 5)
            }
            ScrollView(.horizontal, showsIndicators: false) {
                if lang?.lowercased() == "diff" {
                    diffBody
                } else {
                    Text(code)
                        .font(.system(size: baseSize, design: .monospaced))
                        .foregroundStyle(Konjo.ok)
                        .textSelection(.enabled)
                        .padding(8)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.black.opacity(0.4))
        .clipShape(RoundedRectangle(cornerRadius: 5))
        .overlay(RoundedRectangle(cornerRadius: 5).stroke(Konjo.line2, lineWidth: 1))
    }

    /// Green/red gutter coloring for a `diff` fence (added/removed/hunk lines).
    private var diffBody: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(code.components(separatedBy: "\n").enumerated()), id: \.offset) { _, line in
                Text(line.isEmpty ? " " : line)
                    .font(.system(size: baseSize, design: .monospaced))
                    .foregroundStyle(diffColor(line))
                    .padding(.horizontal, 8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(diffBackground(line))
            }
        }
        .padding(.vertical, 6)
        .textSelection(.enabled)
    }

    private func diffColor(_ line: String) -> Color {
        if line.hasPrefix("+"), !line.hasPrefix("+++") { return Color(.sRGB, red: 0.25, green: 0.73, blue: 0.31) }
        if line.hasPrefix("-"), !line.hasPrefix("---") { return Color(.sRGB, red: 0.97, green: 0.32, blue: 0.29) }
        if line.hasPrefix("@@") { return Konjo.ice }
        return Konjo.fgDim
    }

    private func diffBackground(_ line: String) -> Color {
        if line.hasPrefix("+"), !line.hasPrefix("+++") { return Color(.sRGB, red: 0.18, green: 0.63, blue: 0.26).opacity(0.15) }
        if line.hasPrefix("-"), !line.hasPrefix("---") { return Color(.sRGB, red: 0.97, green: 0.32, blue: 0.29).opacity(0.15) }
        return .clear
    }
}

/// One parsed markdown block from a log line.
enum MarkdownLogBlock: Equatable {
    case paragraph(String)
    case heading(String, level: Int)
    case bullets([String])
    case code(String, lang: String?)

    /// Split raw text into ordered blocks. Fenced ``` regions become `.code`;
    /// `#`-prefixed lines become `.heading`; runs of `- `/`* ` lines collapse
    /// into one `.bullets`; consecutive plain lines join into a `.paragraph`
    /// (blank lines separate paragraphs).
    static func parse(_ raw: String) -> [MarkdownLogBlock] {
        var ctx = ParseContext()
        for line in raw.components(separatedBy: "\n") {
            ctx.consume(line)
        }
        ctx.finish()
        return ctx.blocks
    }
}

/// Mutable accumulator for [`MarkdownLogBlock.parse`], kept separate so each
/// step stays small and the state transitions are explicit.
private struct ParseContext {
    var blocks: [MarkdownLogBlock] = []
    private var paragraph: [String] = []
    private var bullets: [String] = []
    private var code: [String] = []
    private var inCode = false
    private var codeLang: String?

    mutating func consume(_ line: String) {
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        if trimmed.hasPrefix("```") {
            toggleFence(trimmed)
        } else if inCode {
            code.append(line)
        } else if trimmed.isEmpty {
            flushParagraph(); flushBullets()
        } else if trimmed.hasPrefix("#") {
            consumeHeading(trimmed)
        } else if trimmed.hasPrefix("- ") || trimmed.hasPrefix("* ") {
            flushParagraph()
            bullets.append(String(trimmed.dropFirst(2)))
        } else {
            flushBullets()
            paragraph.append(trimmed)
        }
    }

    mutating func finish() {
        if inCode, !code.isEmpty {
            blocks.append(.code(code.joined(separator: "\n"), lang: codeLang))
        }
        flushParagraph(); flushBullets()
    }

    private mutating func toggleFence(_ trimmed: String) {
        if inCode {
            blocks.append(.code(code.joined(separator: "\n"), lang: codeLang))
            code.removeAll(); codeLang = nil; inCode = false
        } else {
            flushParagraph(); flushBullets()
            let lang = trimmed.dropFirst(3).trimmingCharacters(in: .whitespaces)
            codeLang = lang.isEmpty ? nil : lang
            inCode = true
        }
    }

    private mutating func consumeHeading(_ trimmed: String) {
        flushParagraph(); flushBullets()
        let hashes = trimmed.prefix { $0 == "#" }.count
        let title = trimmed.drop { $0 == "#" }.trimmingCharacters(in: .whitespaces)
        blocks.append(.heading(title, level: hashes))
    }

    private mutating func flushParagraph() {
        guard !paragraph.isEmpty else { return }
        blocks.append(.paragraph(paragraph.joined(separator: "\n")))
        paragraph.removeAll()
    }

    private mutating func flushBullets() {
        guard !bullets.isEmpty else { return }
        blocks.append(.bullets(bullets))
        bullets.removeAll()
    }
}
