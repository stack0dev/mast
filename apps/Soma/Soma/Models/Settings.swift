import Foundation
import SwiftData

@Model
final class Settings {
    var llmProvider: String // "apple", "openai", "claude"
    var openAIKey: String
    var claudeKey: String
    var openAIModel: String
    var claudeModel: String
    var theme: String // "system", "light", "dark"
    var hasCompletedOnboarding: Bool

    init() {
        self.llmProvider = "apple"
        self.openAIKey = ""
        self.claudeKey = ""
        self.openAIModel = "gpt-4o-mini"
        self.claudeModel = "claude-sonnet-4-5-20250929"
        self.theme = "system"
        self.hasCompletedOnboarding = false
    }
}
