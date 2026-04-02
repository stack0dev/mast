import SwiftUI
import Charts

struct MoodChartView: View {
    let moods: [MoodRecord]

    /// Moods filtered to the last 30 days and sorted chronologically.
    private var recentMoods: [MoodRecord] {
        let cutoff = Calendar.current.date(byAdding: .day, value: -30, to: .now) ?? .now
        return moods
            .filter { $0.date >= cutoff }
            .sorted { $0.date < $1.date }
    }

    private func numericMood(_ name: String) -> Double {
        (Mood(rawValue: name) ?? .neutral).numericValue
    }

    private func moodLabel(_ value: Double) -> String {
        switch value {
        case 5: "Happy"
        case 4: "Calm"
        case 3: "Neutral"
        case 2: "Sad"
        case 1: "Anxious"
        case 0: "Angry"
        default: ""
        }
    }

    var body: some View {
        if recentMoods.isEmpty {
            ContentUnavailableView {
                Label("No Mood Data", systemImage: "chart.xyaxis.line")
            } description: {
                Text("Mood tracking begins when you save journal entries. Your moods over the last 30 days will appear here.")
            }
        } else {
            chart
                .padding()
        }
    }

    // MARK: - Chart

    private var chart: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Mood Over 30 Days")
                .font(.headline)

            Chart(recentMoods, id: \.date) { record in
                // Area fill
                AreaMark(
                    x: .value("Date", record.date),
                    y: .value("Mood", numericMood(record.mood))
                )
                .foregroundStyle(
                    .linearGradient(
                        colors: [.blue.opacity(0.3), .blue.opacity(0.05)],
                        startPoint: .top,
                        endPoint: .bottom
                    )
                )
                .interpolationMethod(.catmullRom)

                // Line
                LineMark(
                    x: .value("Date", record.date),
                    y: .value("Mood", numericMood(record.mood))
                )
                .foregroundStyle(.blue)
                .interpolationMethod(.catmullRom)
                .lineStyle(StrokeStyle(lineWidth: 2))

                // Points
                PointMark(
                    x: .value("Date", record.date),
                    y: .value("Mood", numericMood(record.mood))
                )
                .foregroundStyle(
                    (Mood(rawValue: record.mood) ?? .neutral).color
                )
                .symbolSize(30)
                .annotation(position: .top, spacing: 4) {
                    Text((Mood(rawValue: record.mood) ?? .neutral).emoji)
                        .font(.caption2)
                }
            }
            .chartYScale(domain: 0...5)
            .chartYAxis {
                AxisMarks(values: [0, 1, 2, 3, 4, 5]) { value in
                    AxisGridLine()
                    AxisValueLabel {
                        Text(moodLabel(value.as(Double.self) ?? 0))
                            .font(.caption2)
                    }
                }
            }
            .chartXAxis {
                AxisMarks(values: .stride(by: .day, count: 7)) { _ in
                    AxisGridLine()
                    AxisValueLabel(format: .dateTime.month(.abbreviated).day())
                }
            }
            .frame(minHeight: 300)
        }
    }
}
