import Foundation

enum MarkdownHelper {

    /// Strip markdown formatting for embedding (plain text only).
    static func stripFormatting(_ markdown: String) -> String {
        var text = markdown

        // Remove code blocks (must come before inline patterns)
        text = text.replacingOccurrences(
            of: "```[\\s\\S]*?```",
            with: "",
            options: .regularExpression
        )

        // Remove headers
        text = text.replacingOccurrences(
            of: "(?m)^#{1,6}\\s+",
            with: "",
            options: .regularExpression
        )

        // Remove images ![alt](url) -> alt
        text = text.replacingOccurrences(
            of: "!\\[([^\\]]*?)\\]\\([^)]+\\)",
            with: "$1",
            options: .regularExpression
        )

        // Remove links [text](url) -> text
        text = text.replacingOccurrences(
            of: "\\[([^\\]]*?)\\]\\([^)]+\\)",
            with: "$1",
            options: .regularExpression
        )

        // Remove bold/italic (***text***, **text**, *text*)
        text = text.replacingOccurrences(
            of: "\\*{1,3}(.+?)\\*{1,3}",
            with: "$1",
            options: .regularExpression
        )

        // Remove inline code
        text = text.replacingOccurrences(
            of: "`(.+?)`",
            with: "$1",
            options: .regularExpression
        )

        // Remove horizontal rules
        text = text.replacingOccurrences(
            of: "(?m)^-{3,}$",
            with: "",
            options: .regularExpression
        )

        // Remove blockquotes
        text = text.replacingOccurrences(
            of: "(?m)^>\\s?",
            with: "",
            options: .regularExpression
        )

        // Remove unordered list markers
        text = text.replacingOccurrences(
            of: "(?m)^\\s*[-*+]\\s+",
            with: "",
            options: .regularExpression
        )

        // Remove ordered list markers
        text = text.replacingOccurrences(
            of: "(?m)^\\s*\\d+\\.\\s+",
            with: "",
            options: .regularExpression
        )

        // Collapse excessive newlines
        text = text.replacingOccurrences(
            of: "\n{3,}",
            with: "\n\n",
            options: .regularExpression
        )

        return text.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Create an AttributedString from markdown for display.
    static func render(_ markdown: String) -> AttributedString {
        (try? AttributedString(markdown: markdown)) ?? AttributedString(markdown)
    }
}
