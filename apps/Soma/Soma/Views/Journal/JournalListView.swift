import SwiftUI
import SwiftData

// MARK: - Mood Helpers

enum Mood: String, CaseIterable, Identifiable {
    case happy, sad, anxious, calm, angry, neutral

    var id: String { rawValue }

    var emoji: String {
        switch self {
        case .happy: "😊"
        case .sad: "😢"
        case .anxious: "😰"
        case .calm: "😌"
        case .angry: "😠"
        case .neutral: "😐"
        }
    }

    var color: Color {
        switch self {
        case .happy: .yellow
        case .sad: .blue
        case .anxious: .purple
        case .calm: .green
        case .angry: .red
        case .neutral: .gray
        }
    }

    var numericValue: Double {
        switch self {
        case .happy: 5
        case .calm: 4
        case .neutral: 3
        case .sad: 2
        case .anxious: 1
        case .angry: 0
        }
    }
}

// MARK: - Journal List View

struct JournalListView: View {
    @Environment(MastBridge.self) private var bridge
    @Environment(\.modelContext) private var modelContext
    @State private var viewModel: JournalViewModel?
    @State private var showEditor = false
    @State private var searchText = ""
    @State private var searchTask: Task<Void, Never>?

    private var embedder: AppleEmbedder {
        AppleEmbedder()
    }

    var body: some View {
        Group {
            if let viewModel {
                journalContent(viewModel: viewModel)
            } else {
                ProgressView("Loading...")
            }
        }
        .task {
            let vm = JournalViewModel(bridge: bridge)
            viewModel = vm
            await vm.loadEntries()
        }
    }

    @ViewBuilder
    private func journalContent(viewModel: JournalViewModel) -> some View {
        List {
            if viewModel.isSearching, !viewModel.searchResults.isEmpty {
                searchResultsSection(viewModel: viewModel)
            } else if viewModel.isSearching, viewModel.searchResults.isEmpty {
                ContentUnavailableView.search(text: searchText)
            } else if viewModel.entries.isEmpty {
                emptyState
            } else {
                entriesSection(viewModel: viewModel)
            }
        }
        .searchable(text: $searchText, prompt: "Search journal entries...")
        .onChange(of: searchText) { _, newValue in
            viewModel.searchQuery = newValue
            searchTask?.cancel()
            if newValue.trimmingCharacters(in: .whitespaces).isEmpty {
                viewModel.isSearching = false
                viewModel.searchResults = []
                return
            }
            searchTask = Task {
                try? await Task.sleep(for: .milliseconds(300))
                guard !Task.isCancelled else { return }
                let emb = embedder
                await viewModel.search(
                    embed: { text in await emb.embed(text) },
                    dimensions: emb.dimensions
                )
            }
        }
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button {
                    showEditor = true
                } label: {
                    Label("New Entry", systemImage: "square.and.pencil")
                }
                .keyboardShortcut("n", modifiers: .command)
            }
        }
        .navigationTitle("Journal")
        .sheet(isPresented: $showEditor) {
            JournalEditorView(viewModel: viewModel)
        }
        .overlay {
            if viewModel.isLoading {
                ProgressView()
            }
        }
        .alert("Error", isPresented: .init(
            get: { viewModel.error != nil },
            set: { if !$0 { viewModel.error = nil } }
        )) {
            Button("OK") { viewModel.error = nil }
        } message: {
            Text(viewModel.error ?? "")
        }
    }

    // MARK: - Sections

    @ViewBuilder
    private var emptyState: some View {
        ContentUnavailableView {
            Label("No Journal Entries", systemImage: "book.closed")
        } description: {
            Text("Start writing to capture your thoughts and track your emotional journey.")
        } actions: {
            Button("Write Your First Entry") {
                showEditor = true
            }
            .buttonStyle(.borderedProminent)
        }
        .listRowSeparator(.hidden)
        .listRowBackground(Color.clear)
    }

    @ViewBuilder
    private func entriesSection(viewModel: JournalViewModel) -> some View {
        let grouped = Dictionary(grouping: viewModel.entries) { entry in
            Calendar.current.startOfDay(for: entry.createdAt)
        }
        let sortedDates = grouped.keys.sorted(by: >)

        ForEach(sortedDates, id: \.self) { date in
            Section {
                ForEach(grouped[date] ?? []) { entry in
                    JournalEntryRow(entry: entry)
                        .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                            Button(role: .destructive) {
                                Task {
                                    try? await viewModel.deleteEntry(id: entry.id)
                                }
                            } label: {
                                Label("Delete", systemImage: "trash")
                            }
                        }
                }
            } header: {
                Text(date, style: .date)
            }
        }
    }

    @ViewBuilder
    private func searchResultsSection(viewModel: JournalViewModel) -> some View {
        Section("Search Results") {
            ForEach(viewModel.searchResults) { result in
                SearchResultRow(result: result)
            }
        }
    }
}

// MARK: - Journal Entry Row

private struct JournalEntryRow: View {
    let entry: MastMemory

    private var mood: Mood {
        Mood(rawValue: entry.metadata["mood"] ?? "neutral") ?? .neutral
    }

    private var preview: String {
        let content = entry.metadata["raw_markdown"] ?? entry.content
        let stripped = content.replacingOccurrences(of: "\n", with: " ")
        if stripped.count > 120 {
            return String(stripped.prefix(120)) + "..."
        }
        return stripped
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(entry.createdAt, style: .time)
                    .font(.caption)
                    .foregroundStyle(.secondary)

                Spacer()

                MoodPill(mood: mood)

                if let exerciseType = entry.metadata["exercise_type"] {
                    ExerciseTypeBadge(type: exerciseType)
                }
            }

            Text(preview)
                .font(.body)
                .lineLimit(3)
                .foregroundStyle(.primary)
        }
        .padding(.vertical, 4)
    }
}

// MARK: - Search Result Row

private struct SearchResultRow: View {
    let result: MastRecallResult

    private var mood: Mood {
        Mood(rawValue: result.metadata["mood"] ?? "neutral") ?? .neutral
    }

    private var preview: String {
        let content = result.metadata["raw_markdown"] ?? result.content
        let stripped = content.replacingOccurrences(of: "\n", with: " ")
        if stripped.count > 120 {
            return String(stripped.prefix(120)) + "..."
        }
        return stripped
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(result.createdAt, style: .date)
                    .font(.caption)
                    .foregroundStyle(.secondary)

                Text(result.createdAt, style: .time)
                    .font(.caption)
                    .foregroundStyle(.secondary)

                Spacer()

                MoodPill(mood: mood)

                Text(String(format: "%.0f%%", result.score * 100))
                    .font(.caption2)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 2)
                    .background(.tint.opacity(0.12), in: Capsule())
            }

            Text(preview)
                .font(.body)
                .lineLimit(3)
                .foregroundStyle(.primary)
        }
        .padding(.vertical, 4)
    }
}

// MARK: - Reusable Components

struct MoodPill: View {
    let mood: Mood

    var body: some View {
        HStack(spacing: 3) {
            Text(mood.emoji)
                .font(.caption)
            Text(mood.rawValue.capitalized)
                .font(.caption2)
                .fontWeight(.medium)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 3)
        .background(mood.color.opacity(0.15), in: Capsule())
    }
}

private struct ExerciseTypeBadge: View {
    let type: String

    private var label: String {
        switch type {
        case "thought_record": "CBT"
        case "gratitude": "Gratitude"
        case "breathing": "Breathing"
        default: type.capitalized
        }
    }

    var body: some View {
        Text(label)
            .font(.caption2)
            .fontWeight(.medium)
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(.indigo.opacity(0.12), in: Capsule())
    }
}
