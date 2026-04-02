import Foundation

// MARK: - Tier

enum MastTier: String, CaseIterable, Sendable {
    case core
    case active
    case background
    case archive
}

// MARK: - Memory

struct MastMemory: Identifiable, Sendable {
    let id: String
    let content: String
    let tier: MastTier
    let createdAt: Date
    let accessCount: UInt64
    let metadata: [String: String]
}

// MARK: - Recall Result

struct MastRecallResult: Identifiable, Sendable {
    let id: String
    let content: String
    let score: Float
    let tier: MastTier
    let createdAt: Date
    let accessCount: UInt64
    let metadata: [String: String]
}

// MARK: - Edge

struct MastEdge: Identifiable, Sendable {
    let id: String
    let source: String
    let target: String
    let relation: String
    let weight: Float
    let createdAt: Date
    let updatedAt: Date
}

// MARK: - Compact Result

struct MastCompactResult: Sendable {
    let sourceCount: Int
    let compactedCount: Int
    let newIds: [String]
    let sourceIds: [String]
}

// MARK: - Search Mode

enum MastSearchMode: Sendable {
    case vector
    case fullText
    case hybrid(vectorWeight: Float, textWeight: Float)

    var cString: String {
        switch self {
        case .vector: "vector"
        case .fullText: "fulltext"
        case .hybrid: "hybrid"
        }
    }

    var vectorWeight: Float {
        switch self {
        case .hybrid(let w, _): w
        default: 0.0
        }
    }

    var textWeight: Float {
        switch self {
        case .hybrid(_, let w): w
        default: 0.0
        }
    }
}

// MARK: - Compaction Strategy

enum MastCompactionStrategy: Sendable {
    case age(seconds: Int64)
    case accessDecay
    case hybrid
    case manual(ids: [String])

    var cString: String {
        switch self {
        case .age(let secs): "age:\(secs)"
        case .accessDecay: "access_decay"
        case .hybrid: "hybrid"
        case .manual(let ids): "manual:\(ids.joined(separator: ","))"
        }
    }
}
