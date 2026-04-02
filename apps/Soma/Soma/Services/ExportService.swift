import Foundation
import AppKit

@Observable
final class ExportService {
    private let bridge: MastBridge

    init(bridge: MastBridge) {
        self.bridge = bridge
    }

    func exportPDF(memories: [MastMemory], to url: URL) throws {
        let text = memories.map { memory in
            let dateStr = memory.createdAt.formatted(date: .long, time: .shortened)
            let mood = memory.metadata["mood"] ?? "---"
            return "Date: \(dateStr)\nMood: \(mood)\n\n\(memory.metadata["raw_markdown"] ?? memory.content)\n\n---\n"
        }.joined()

        let textView = NSTextView(frame: NSRect(x: 0, y: 0, width: 468, height: 648))
        textView.string = text
        textView.font = .systemFont(ofSize: 11)

        let printInfo = NSPrintInfo()
        printInfo.paperSize = NSSize(width: 612, height: 792) // US Letter
        printInfo.topMargin = 72
        printInfo.bottomMargin = 72
        printInfo.leftMargin = 72
        printInfo.rightMargin = 72
        printInfo.jobDisposition = .save
        printInfo.dictionary()[NSPrintInfo.AttributeKey.jobSavingURL] = url

        let printOp = NSPrintOperation(view: textView, printInfo: printInfo)
        printOp.showsPrintPanel = false
        printOp.showsProgressPanel = false
        printOp.run()
    }

    func exportSnapshot(
        collection: String,
        to url: URL
    ) async throws -> (memoryCount: UInt64, edgeCount: UInt64) {
        try await bridge.snapshot(collection: collection, outputPath: url.path)
    }
}
