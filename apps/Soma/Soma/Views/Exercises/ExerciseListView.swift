import SwiftUI

// MARK: - Exercise Definition

private struct ExerciseDefinition: Identifiable {
    let id: String
    let title: String
    let subtitle: String
    let icon: String
    let color: Color
}

private let exercises: [ExerciseDefinition] = [
    ExerciseDefinition(
        id: "thought_record",
        title: "Thought Record",
        subtitle: "CBT-based cognitive restructuring. Examine automatic thoughts and find balanced alternatives.",
        icon: "brain.head.profile",
        color: .indigo
    ),
    ExerciseDefinition(
        id: "breathing",
        title: "Box Breathing",
        subtitle: "Guided 4-4-4-4 breathing exercise to calm your nervous system.",
        icon: "wind",
        color: .teal
    ),
    ExerciseDefinition(
        id: "gratitude",
        title: "Gratitude Journal",
        subtitle: "Reflect on what you are grateful for with a guided prompt.",
        icon: "heart.text.square",
        color: .orange
    ),
]

// MARK: - Exercise List View

struct ExerciseListView: View {
    @State private var selectedExercise: String?

    var body: some View {
        ScrollView {
            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 240, maximum: 320), spacing: 16)],
                spacing: 16
            ) {
                ForEach(exercises) { exercise in
                    ExerciseCard(exercise: exercise) {
                        selectedExercise = exercise.id
                    }
                }
            }
            .padding()
        }
        .navigationTitle("Exercises")
        .sheet(item: $selectedExercise) { exerciseId in
            switch exerciseId {
            case "thought_record":
                ThoughtRecordView()
            case "breathing":
                BreathingView()
            case "gratitude":
                GratitudeView()
            default:
                Text("Unknown exercise")
            }
        }
    }
}

// MARK: - String + Identifiable for sheet binding

extension String: @retroactive Identifiable {
    public var id: String { self }
}

// MARK: - Exercise Card

private struct ExerciseCard: View {
    let exercise: ExerciseDefinition
    let action: () -> Void

    @State private var isHovering = false

    var body: some View {
        Button(action: action) {
            VStack(alignment: .leading, spacing: 12) {
                HStack {
                    Image(systemName: exercise.icon)
                        .font(.title2)
                        .foregroundStyle(exercise.color)
                        .frame(width: 40, height: 40)
                        .background(exercise.color.opacity(0.12), in: RoundedRectangle(cornerRadius: 8))

                    Spacer()

                    Image(systemName: "chevron.right")
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                }

                Text(exercise.title)
                    .font(.headline)
                    .foregroundStyle(.primary)

                Text(exercise.subtitle)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(3)
            }
            .padding()
            .frame(maxWidth: .infinity, alignment: .leading)
            .background {
                RoundedRectangle(cornerRadius: 12)
                    .fill(.background)
                    .shadow(color: .black.opacity(isHovering ? 0.1 : 0.05), radius: isHovering ? 8 : 4)
            }
            .overlay {
                RoundedRectangle(cornerRadius: 12)
                    .strokeBorder(.quaternary, lineWidth: 0.5)
            }
            .scaleEffect(isHovering ? 1.02 : 1.0)
            .animation(.snappy(duration: 0.2), value: isHovering)
        }
        .buttonStyle(.plain)
        .onHover { hovering in
            isHovering = hovering
        }
    }
}
