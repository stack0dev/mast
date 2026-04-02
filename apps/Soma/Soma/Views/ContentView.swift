import SwiftUI
import SwiftData

// MARK: - Sidebar Navigation

enum SidebarItem: String, CaseIterable, Identifiable {
    case journal = "Journal"
    case insights = "Insights"
    case exercises = "Exercises"
    case settings = "Settings"

    var id: String { rawValue }

    var icon: String {
        switch self {
        case .journal: "book"
        case .insights: "chart.bar"
        case .exercises: "heart.circle"
        case .settings: "gear"
        }
    }
}

// MARK: - Content View

struct ContentView: View {
    @State private var selectedItem: SidebarItem? = .journal
    @State private var showOnboarding = false
    @Environment(MastBridge.self) private var bridge
    @Environment(\.modelContext) private var modelContext

    var body: some View {
        NavigationSplitView {
            List(SidebarItem.allCases, selection: $selectedItem) { item in
                Label(item.rawValue, systemImage: item.icon)
                    .tag(item)
            }
            .navigationSplitViewColumnWidth(min: 180, ideal: 200, max: 250)
            .listStyle(.sidebar)
        } detail: {
            switch selectedItem {
            case .journal:
                JournalListView()
            case .insights:
                PatternInsightsView()
            case .exercises:
                ExerciseListView()
            case .settings:
                SettingsView()
            case nil:
                ContentUnavailableView(
                    "Select an item",
                    systemImage: "sidebar.left",
                    description: Text("Choose a section from the sidebar to get started.")
                )
            }
        }
        .task {
            let descriptor = FetchDescriptor<Settings>()
            let settings = try? modelContext.fetch(descriptor).first
            if settings == nil || !(settings?.hasCompletedOnboarding ?? false) {
                showOnboarding = true
            }
        }
        .sheet(isPresented: $showOnboarding) {
            OnboardingView()
        }
    }
}
