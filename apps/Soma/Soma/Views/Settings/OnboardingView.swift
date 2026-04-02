import SwiftUI
import SwiftData

struct OnboardingView: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(\.modelContext) private var modelContext
    @Query private var allSettings: [Settings]

    @State private var currentPage = 0

    private let pages: [(icon: String, title: String, body: String)] = [
        (
            "book.fill",
            "Your Private Journal",
            "Write freely. Soma keeps your thoughts private and secure on your device. Your entries are never sent to external servers unless you choose a cloud AI provider."
        ),
        (
            "brain.head.profile.fill",
            "AI-Powered Reflection",
            "After each entry, receive gentle, compassionate reflections that draw connections to your past experiences. Works out of the box with Apple Intelligence -- no API key needed."
        ),
        (
            "chart.bar.fill",
            "Track Patterns Over Time",
            "Mood charts, trigger graphs, and pattern detection help you understand your emotional landscape. The more you write, the deeper the insights."
        ),
        (
            "heart.circle.fill",
            "Therapeutic Exercises",
            "CBT thought records, box breathing, and gratitude journaling -- evidence-based tools to support your mental well-being."
        ),
    ]

    var body: some View {
        VStack(spacing: 0) {
            // Page content
            TabView(selection: $currentPage) {
                ForEach(Array(pages.enumerated()), id: \.offset) { index, page in
                    pageView(icon: page.icon, title: page.title, body: page.body)
                        .tag(index)
                }
            }
            .tabViewStyle(.automatic)
            .frame(minHeight: 320)

            Divider()

            // Bottom bar
            HStack {
                // Page indicator
                HStack(spacing: 6) {
                    ForEach(0..<pages.count, id: \.self) { index in
                        Circle()
                            .fill(currentPage == index ? Color.accentColor : Color.secondary.opacity(0.3))
                            .frame(width: 7, height: 7)
                    }
                }

                Spacer()

                if currentPage < pages.count - 1 {
                    Button("Next") {
                        withAnimation {
                            currentPage += 1
                        }
                    }
                    .buttonStyle(.borderedProminent)
                } else {
                    Button("Get Started") {
                        completeOnboarding()
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.large)
                }
            }
            .padding()
        }
        .frame(width: 520, height: 440)
        .interactiveDismissDisabled()
    }

    // MARK: - Page View

    private func pageView(icon: String, title: String, body: String) -> some View {
        VStack(spacing: 20) {
            Spacer()

            Image(systemName: icon)
                .font(.system(size: 48))
                .foregroundStyle(.tint)
                .padding(.bottom, 4)

            Text(title)
                .font(.title2)
                .fontWeight(.semibold)
                .multilineTextAlignment(.center)

            Text(body)
                .font(.body)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .lineSpacing(4)
                .frame(maxWidth: 380)

            Spacer()
        }
        .padding(.horizontal, 40)
    }

    // MARK: - Complete

    private func completeOnboarding() {
        let settings: Settings
        if let existing = allSettings.first {
            settings = existing
        } else {
            settings = Settings()
            modelContext.insert(settings)
        }
        settings.hasCompletedOnboarding = true
        try? modelContext.save()
        dismiss()
    }
}
