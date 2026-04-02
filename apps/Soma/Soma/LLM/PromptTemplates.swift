import Foundation

// MARK: - Prompt Templates

enum PromptTemplates {

    // MARK: - Reflection

    static func reflection(entry: String, relatedMemories: [String]) -> (system: String, user: String) {
        let system = """
        You are a compassionate therapeutic journal companion. Your role is to offer gentle, \
        insightful reflections on the user's journal entries. Draw connections to their past \
        entries when relevant. Never diagnose or prescribe. Use empathetic, warm language. \
        Keep responses to 2-3 paragraphs.
        """

        let memoriesContext: String
        if relatedMemories.isEmpty {
            memoriesContext = ""
        } else {
            let formatted = relatedMemories.enumerated()
                .map { "- Entry \($0.offset + 1): \($0.element)" }
                .joined(separator: "\n")
            memoriesContext = """

            Here are some of their related past entries for context:
            \(formatted)
            """
        }

        let user = """
        Please reflect on this journal entry:\(memoriesContext)

        Today's entry:
        \(entry)
        """

        return (system, user)
    }

    // MARK: - Cognitive Reframing

    static func reframe(thought: String) -> (system: String, user: String) {
        let system = """
        You are a CBT-trained therapeutic companion. Help the user examine their thought \
        pattern and gently suggest alternative perspectives. Use the ABC model (Activating \
        event, Belief, Consequence). Be warm and non-judgmental. Keep it to 2-3 paragraphs.
        """

        let user = "Help me reframe this thought: \(thought)"

        return (system, user)
    }

    // MARK: - Pattern Summary

    static func patternSummary(entries: [String]) -> (system: String, user: String) {
        let system = """
        You are analyzing a series of journal entries to identify emotional patterns, \
        recurring themes, and growth areas. Produce a concise summary (2-3 paragraphs) \
        that highlights: 1) Key emotional patterns, 2) Common triggers, 3) Signs of \
        growth or areas needing attention. Be compassionate and constructive.
        """

        let formatted = entries.enumerated()
            .map { "Entry \($0.offset + 1): \($0.element)" }
            .joined(separator: "\n\n")

        let user = """
        Summarize the patterns in these journal entries:
        \(formatted)
        """

        return (system, user)
    }

    // MARK: - Trigger Extraction

    static func triggerExtraction(entry: String) -> (system: String, user: String) {
        let system = """
        You are an emotion analysis assistant. Extract trigger-emotion pairs from journal \
        entries. Return ONLY a JSON array with objects containing: "trigger" (the external \
        event or situation), "emotion" (the feeling it caused), "intensity" (0.0 to 1.0). \
        Keep triggers as simple noun phrases. Keep emotions as single words. Example: \
        [{"trigger": "deadline", "emotion": "anxiety", "intensity": 0.8}]
        """

        let user = "Extract triggers and emotions from this entry:\n\(entry)"

        return (system, user)
    }

    // MARK: - Gratitude Prompts

    private static let gratitudePrompts = [
        "What's something small that brought you joy today?",
        "Who made a positive difference in your day?",
        "What's something you're grateful for that you usually take for granted?",
        "What ability or skill are you thankful to have?",
        "What's a challenge that taught you something valuable recently?",
        "What's something beautiful you noticed today?",
        "What's a comfort in your life you appreciate?",
    ]

    static func gratitudePrompt() -> String {
        gratitudePrompts.randomElement() ?? gratitudePrompts[0]
    }
}
