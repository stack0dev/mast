import SwiftUI
import SwiftData

struct PatternInsightsView: View {
    @Environment(MastBridge.self) private var bridge
    @Environment(\.modelContext) private var modelContext
    @State private var viewModel: InsightsViewModel?
    @State private var selectedTab = InsightsTab.patterns

    enum InsightsTab: String, CaseIterable, Identifiable {
        case patterns = "Patterns"
        case moodChart = "Mood Chart"
        case triggerGraph = "Trigger Graph"

        var id: String { rawValue }

        var icon: String {
            switch self {
            case .patterns: "brain.head.profile"
            case .moodChart: "chart.xyaxis.line"
            case .triggerGraph: "point.3.connected.trianglepath.dotted"
            }
        }
    }

    var body: some View {
        Group {
            if let viewModel {
                insightsContent(viewModel: viewModel)
            } else {
                ProgressView("Loading insights...")
            }
        }
        .navigationTitle("Insights")
        .task {
            let vm = InsightsViewModel(bridge: bridge)
            viewModel = vm
            await vm.load(modelContext: modelContext)
        }
    }

    @ViewBuilder
    private func insightsContent(viewModel: InsightsViewModel) -> some View {
        VStack(spacing: 0) {
            // Tab bar
            Picker("View", selection: $selectedTab) {
                ForEach(InsightsTab.allCases) { tab in
                    Label(tab.rawValue, systemImage: tab.icon)
                        .tag(tab)
                }
            }
            .pickerStyle(.segmented)
            .padding()

            Divider()

            // Tab content
            switch selectedTab {
            case .patterns:
                patternsTab(viewModel: viewModel)
            case .moodChart:
                MoodChartView(moods: viewModel.moods)
            case .triggerGraph:
                TriggerGraphView(edges: viewModel.edges)
            }
        }
        .overlay {
            if viewModel.isLoading {
                ProgressView()
            }
        }
    }

    // MARK: - Patterns Tab

    @ViewBuilder
    private func patternsTab(viewModel: InsightsViewModel) -> some View {
        if viewModel.patterns.isEmpty {
            ContentUnavailableView {
                Label("No Patterns Yet", systemImage: "brain.head.profile")
            } description: {
                Text("Patterns are detected by analyzing and compacting your journal entries over time. Keep writing to discover emotional themes.")
            }
        } else {
            List(viewModel.patterns) { pattern in
                VStack(alignment: .leading, spacing: 6) {
                    HStack {
                        Image(systemName: "sparkles")
                            .foregroundStyle(.orange)
                        Text(pattern.createdAt, style: .date)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }

                    Text(pattern.content)
                        .font(.body)
                        .lineLimit(6)
                        .textSelection(.enabled)
                }
                .padding(.vertical, 4)
            }
        }
    }
}
