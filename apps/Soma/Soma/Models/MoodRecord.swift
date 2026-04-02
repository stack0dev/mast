import Foundation
import SwiftData

@Model
final class MoodRecord {
    var mood: String // "happy", "sad", "anxious", "calm", "angry", "neutral"
    var date: Date
    var entryId: String // ULID of the corresponding mast memory
    var notes: String

    init(mood: String, date: Date = .now, entryId: String, notes: String = "") {
        self.mood = mood
        self.date = date
        self.entryId = entryId
        self.notes = notes
    }
}
