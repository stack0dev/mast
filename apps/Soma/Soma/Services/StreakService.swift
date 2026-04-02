import Foundation
import SwiftData

@Observable
final class StreakService {
    private let modelContext: ModelContext

    init(modelContext: ModelContext) {
        self.modelContext = modelContext
    }

    func recordEntry() {
        let streak = fetchOrCreateStreak()
        let calendar = Calendar.current
        let today = calendar.startOfDay(for: .now)

        if let lastDate = streak.lastEntryDate {
            let lastDay = calendar.startOfDay(for: lastDate)
            let daysBetween = calendar.dateComponents([.day], from: lastDay, to: today).day ?? 0

            if daysBetween == 1 {
                streak.currentStreak += 1
            } else if daysBetween > 1 {
                streak.currentStreak = 1
            }
            // daysBetween == 0: same day, no change
        } else {
            streak.currentStreak = 1
        }

        streak.longestStreak = max(streak.longestStreak, streak.currentStreak)
        streak.lastEntryDate = .now
        try? modelContext.save()
    }

    func currentStreak() -> Streak {
        fetchOrCreateStreak()
    }

    private func fetchOrCreateStreak() -> Streak {
        let descriptor = FetchDescriptor<Streak>()
        if let existing = try? modelContext.fetch(descriptor).first {
            return existing
        }
        let streak = Streak()
        modelContext.insert(streak)
        return streak
    }
}
