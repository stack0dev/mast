import SwiftUI
import SwiftData

struct SettingsView: View {
    @Environment(\.modelContext) private var modelContext
    @Environment(MastBridge.self) private var bridge
    @Query private var allSettings: [Settings]

    @State private var showExport = false
    @State private var showImportPanel = false
    @State private var importStatus: String?
    @State private var showImportAlert = false

    private var settings: Settings {
        if let existing = allSettings.first {
            return existing
        }
        let newSettings = Settings()
        modelContext.insert(newSettings)
        try? modelContext.save()
        return newSettings
    }

    var body: some View {
        Form {
            llmSection
            themeSection
            dataSection
        }
        .formStyle(.grouped)
        .navigationTitle("Settings")
        .sheet(isPresented: $showExport) {
            ExportView()
        }
        .alert("Import Complete", isPresented: $showImportAlert) {
            Button("OK") { importStatus = nil }
        } message: {
            Text(importStatus ?? "")
        }
    }

    // MARK: - LLM Provider

    private var llmSection: some View {
        Section {
            Picker("Provider", selection: Binding(
                get: { settings.llmProvider },
                set: { settings.llmProvider = $0; try? modelContext.save() }
            )) {
                Text("Apple Intelligence").tag("apple")
                Text("OpenAI").tag("openai")
                Text("Claude").tag("claude")
            }
            .pickerStyle(.segmented)

            if settings.llmProvider == "openai" {
                SecureField("OpenAI API Key", text: Binding(
                    get: { settings.openAIKey },
                    set: { settings.openAIKey = $0; try? modelContext.save() }
                ))

                Picker("Model", selection: Binding(
                    get: { settings.openAIModel },
                    set: { settings.openAIModel = $0; try? modelContext.save() }
                )) {
                    Text("GPT-4o Mini").tag("gpt-4o-mini")
                    Text("GPT-4o").tag("gpt-4o")
                    Text("GPT-4 Turbo").tag("gpt-4-turbo")
                }
            }

            if settings.llmProvider == "claude" {
                SecureField("Anthropic API Key", text: Binding(
                    get: { settings.claudeKey },
                    set: { settings.claudeKey = $0; try? modelContext.save() }
                ))

                Picker("Model", selection: Binding(
                    get: { settings.claudeModel },
                    set: { settings.claudeModel = $0; try? modelContext.save() }
                )) {
                    Text("Claude Sonnet 4.5").tag("claude-sonnet-4-5-20250929")
                    Text("Claude Sonnet 4").tag("claude-sonnet-4-20250514")
                    Text("Claude Haiku 3.5").tag("claude-3-5-haiku-20241022")
                }
            }
        } header: {
            Label("AI Provider", systemImage: "cpu")
        } footer: {
            if settings.llmProvider == "apple" {
                Text("Apple Intelligence runs entirely on-device. No API key needed.")
            }
        }
    }

    // MARK: - Theme

    private var themeSection: some View {
        Section {
            Picker("Appearance", selection: Binding(
                get: { settings.theme },
                set: { settings.theme = $0; try? modelContext.save() }
            )) {
                Text("System").tag("system")
                Text("Light").tag("light")
                Text("Dark").tag("dark")
            }
            .pickerStyle(.segmented)
        } header: {
            Label("Appearance", systemImage: "paintbrush")
        }
    }

    // MARK: - Data Management

    private var dataSection: some View {
        Section {
            Button {
                showExport = true
            } label: {
                Label("Export Data...", systemImage: "square.and.arrow.up")
            }

            Button {
                importSnapshot()
            } label: {
                Label("Import Snapshot...", systemImage: "square.and.arrow.down")
            }
        } header: {
            Label("Data Management", systemImage: "externaldrive")
        } footer: {
            Text("Export your journal as a MAST snapshot for backup or transfer between devices.")
        }
    }

    // MARK: - Import

    private func importSnapshot() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.json, .plainText]
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = false
        panel.message = "Select a MAST snapshot file to import"
        panel.prompt = "Import"

        guard panel.runModal() == .OK, let url = panel.url else { return }

        Task {
            do {
                let result = try await bridge.restore(inputPath: url.path, merge: true)
                importStatus = "Imported \(result.memoryCount) memories and \(result.edgeCount) edges."
                showImportAlert = true
            } catch {
                importStatus = "Import failed: \(error.localizedDescription)"
                showImportAlert = true
            }
        }
    }
}
