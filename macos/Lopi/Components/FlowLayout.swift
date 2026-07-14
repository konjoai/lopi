import SwiftUI

/// A wrapping horizontal flow — the `Layout` analogue of the web's
/// `display: flex; flex-wrap: wrap; justify-content: flex-start`. Subviews keep
/// their ideal size (CSS `flex: 0 0 auto`) and wrap to the next line when the
/// proposed width runs out.
///
/// Exists because SwiftUI ships no wrapping stack: an `HStack` of config chips
/// would squeeze them past legibility, and a `VStack` (what the config drawer
/// used to be) doesn't match the web at all.
struct FlowLayout: Layout {
    var hSpacing: CGFloat = 6
    var vSpacing: CGFloat = 6

    /// One wrapped line: the subview index range it holds, plus its extent.
    private struct Row {
        var range: Range<Int>
        var width: CGFloat
        var height: CGFloat
    }

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout Void) -> CGSize {
        // A nil or infinite width proposal asks "how big do you want to be?" —
        // answer with the natural single-line width. A finite proposal is a
        // container's width, which a flex container fills (so the drawer's
        // top border spans the whole card, as the CSS border-top does).
        let finite = proposal.width.flatMap { $0.isFinite ? $0 : nil }
        let rows = rows(maxWidth: finite ?? .infinity, subviews: subviews)
        let natural = rows.map(\.width).max() ?? 0
        let height = rows.map(\.height).reduce(0, +) + vSpacing * CGFloat(max(0, rows.count - 1))
        return CGSize(width: finite ?? natural, height: height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout Void) {
        var y = bounds.minY
        for row in rows(maxWidth: bounds.width, subviews: subviews) {
            var x = bounds.minX
            for i in row.range {
                let size = subviews[i].sizeThatFits(.unspecified)
                subviews[i].place(
                    at: CGPoint(x: x, y: y + (row.height - size.height) / 2),
                    proposal: ProposedViewSize(size)
                )
                x += size.width + hSpacing
            }
            y += row.height + vSpacing
        }
    }

    /// Greedy line-breaking over the subviews' ideal sizes. Both passes call
    /// this with the same width, so placement always agrees with the measurement.
    private func rows(maxWidth: CGFloat, subviews: Subviews) -> [Row] {
        var rows: [Row] = []
        var start = 0
        var x: CGFloat = 0
        var height: CGFloat = 0
        for (i, subview) in subviews.enumerated() {
            let size = subview.sizeThatFits(.unspecified)
            let advance = x == 0 ? size.width : x + hSpacing + size.width
            // The `x > 0` guard keeps an over-wide subview on its own line rather
            // than looping forever; the tolerance absorbs float drift.
            if x > 0, advance - maxWidth > 0.5 {
                rows.append(Row(range: start..<i, width: x, height: height))
                start = i
                x = size.width
                height = size.height
            } else {
                x = advance
                height = max(height, size.height)
            }
        }
        if start < subviews.count {
            rows.append(Row(range: start..<subviews.count, width: x, height: height))
        }
        return rows
    }
}
