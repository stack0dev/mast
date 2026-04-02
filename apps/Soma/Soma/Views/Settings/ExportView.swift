import SwiftUI
import SwiftData

struct ExportView: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(MastBridge.self) private var bridge

    @State private var isExporting = false
    @State private var exportResult: String?
    @State private var error: String?

    var body: some View {
        NavigationStack {
            Form {
                formatSection
                actionSection
                resultSection
            }
            .formStyle(.grouped)
            .navigationTitle("Export Data")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Close") { dismiss() }
                }
            }
            .alert("Error", isPresented: .init(
                get: { error != nil },
                set: { if !$0 { error = nil } }
            )) {
                Button("OK") { error = nil }
            } message: {
                Text(error ?? "")
            }
        }
        .frame(minWidth: 420, minHeight: 280)
    }

    // MARK: - Format

    private var formatSection: some View {
        Section {
            HStack {
                Image(systemName: "doc.text")
                    .foregroundStyle(.blue)
                VStack(alignment: .leading, spacing: 2) {
                    Text("MAST Snapshot")
                        .font(.headline)
                    Text("JSONL format containing all memories and edges. Can be imported back into Soma or any MAST-compatible app.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        } header: {
            Label("Format", systemImage: "doc")
        }
    }

    // MARK: - Action

    private var actionSection: some View {
        Section {
            Button {
                exportSnapshot()
            } label: {
                if isExporting {
                    HStack {
                        ProgressView()
                            .controlSize(.small)
                        Text("Exporting...")
                    }
                } else {
                    Label("Export Snapshot...", systemImage: "square.and.arrow.up")
                }
            }
            .disabled(isExporting)
        }
    }

    // MARK: - Result

    @ViewBuilder
    private var resultSection: some View {
        if let result = exportResult {
            Section {
                Label(result, systemImage: "checkmark.circle.fill")
                    .foregroundStyle(.green)
            }
        }
    }

    // MARK: - Export Logic

    private func exportSnapshot() {
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.json]
        panel.nameFieldStringValue = "soma-snapshot-\(formattedDate()).jsonl"
        panel.message = "Choose where to save the snapshot"
        panel.prompt = "Export"

        guard panel.runModal() == .OK, let url = panel.url else { return }

        isExporting = true
        Task {
            do {
                let result = try await bridge.snapshot(
                    collection: "journal",
                    outputPath: url.path
                )
                exportResult = "Exported \(result.memoryCount) memories and \(result.edgeCount) edges."
                isExporting = false
            } catch {
                self.error = error.localizedDescription
                isExporting = false
            }
        }
    }

    private func formattedDate() -> String {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd"
        return formatter.string(from: .now)
    }
}
