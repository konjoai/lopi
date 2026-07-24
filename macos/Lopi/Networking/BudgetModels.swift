import Foundation

/// `GET /api/budget/breakdown` — cost grouped by model (billed today, UTC)
/// and a 7-day daily spend trend, both projected server-side from
/// `turn_metrics` (`crates/lopi-memory/src/store/budget.rs`). No new
/// persistence on the server; this is a read-only rollup.
struct BudgetBreakdown: Codable, Hashable {
    /// One model's spend, billed today (UTC). Highest spend first.
    struct ModelSpend: Codable, Hashable, Identifiable {
        let model: String
        let costUsd: Double
        var id: String { model }

        enum CodingKeys: String, CodingKey {
            case model
            case costUsd = "cost_usd"
        }
    }

    /// One calendar day's spend (UTC), `yyyy-MM-dd`. Zero-filled for days
    /// with no recorded turns.
    struct DaySpend: Codable, Hashable, Identifiable {
        let date: String
        let costUsd: Double
        var id: String { date }

        enum CodingKeys: String, CodingKey {
            case date
            case costUsd = "cost_usd"
        }
    }

    var byModel: [ModelSpend] = []
    /// Oldest first, 7 entries — the last is always today.
    var trend: [DaySpend] = []

    enum CodingKeys: String, CodingKey {
        case byModel = "by_model"
        case trend
    }

    init() {}

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        byModel = try c.decodeIfPresent([ModelSpend].self, forKey: .byModel) ?? []
        trend = try c.decodeIfPresent([DaySpend].self, forKey: .trend) ?? []
    }
}
