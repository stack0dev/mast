import Foundation
import SwiftData

@Model
final class Streak {
    var currentStreak: Int
    var longestStreak: Int
    var lastEntryDate: Date?

    init(currentStreak: Int = 0, longestStreak: Int = 0, lastEntryDate: Date? = nil) {
        self.currentStreak = currentStreak
        self.longestStreak = longestStreak
        self.lastEntryDate = lastEntryDate
    }
}
