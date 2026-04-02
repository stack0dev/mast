import SwiftUI

// MARK: - Breathing Phase

private enum BreathingPhase: CaseIterable {
    case inhale
    case holdIn
    case exhale
    case holdOut

    var instruction: String {
        switch self {
        case .inhale: "Breathe In"
        case .holdIn: "Hold"
        case .exhale: "Breathe Out"
        case .holdOut: "Hold"
        }
    }

    var duration: TimeInterval {
        4.0 // Box breathing: 4 seconds for each phase
    }

    var color: Color {
        switch self {
        case .inhale: .blue
        case .holdIn: .teal
        case .exhale: .indigo
        case .holdOut: .purple
        }
    }

    /// Scale factor for the breathing circle.
    var scale: CGFloat {
        switch self {
        case .inhale: 1.0
        case .holdIn: 1.0
        case .exhale: 0.5
        case .holdOut: 0.5
        }
    }

    var next: BreathingPhase {
        switch self {
        case .inhale: .holdIn
        case .holdIn: .exhale
        case .exhale: .holdOut
        case .holdOut: .inhale
        }
    }
}

// MARK: - Breathing View

struct BreathingView: View {
    @Environment(\.dismiss) private var dismiss

    @State private var isRunning = false
    @State private var phase: BreathingPhase = .inhale
    @State private var roundCount = 0
    @State private var phaseTimeRemaining: TimeInterval = 4.0
    @State private var circleScale: CGFloat = 0.5
    @State private var timer: Timer?
    @State private var totalRounds = 4

    private let tickInterval: TimeInterval = 0.05

    var body: some View {
        NavigationStack {
            VStack(spacing: 40) {
                headerSection

                Spacer()

                breathingCircle

                Spacer()

                controlSection
            }
            .padding(40)
            .navigationTitle("Box Breathing")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Done") { dismiss() }
                }
            }
            .onDisappear {
                stop()
            }
        }
        .frame(minWidth: 400, minHeight: 500)
    }

    // MARK: - Header

    private var headerSection: some View {
        VStack(spacing: 8) {
            Text("Round \(roundCount) of \(totalRounds)")
                .font(.headline)
                .foregroundStyle(.secondary)

            if isRunning {
                Text(phase.instruction)
                    .font(.title)
                    .fontWeight(.medium)
                    .foregroundStyle(phase.color)
                    .contentTransition(.numericText())
                    .animation(.easeInOut(duration: 0.3), value: phase.instruction)

                Text(String(format: "%.0f", ceil(phaseTimeRemaining)))
                    .font(.system(size: 24, weight: .light, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .contentTransition(.numericText())
            } else if roundCount >= totalRounds, roundCount > 0 {
                Text("Session Complete")
                    .font(.title)
                    .fontWeight(.medium)
                    .foregroundStyle(.green)
            } else {
                Text("Ready")
                    .font(.title)
                    .fontWeight(.medium)
                    .foregroundStyle(.secondary)
            }
        }
    }

    // MARK: - Breathing Circle

    private var breathingCircle: some View {
        ZStack {
            // Outer ring
            Circle()
                .stroke(phase.color.opacity(0.15), lineWidth: 3)
                .frame(width: 220, height: 220)

            // Animated inner circle
            Circle()
                .fill(
                    RadialGradient(
                        colors: [phase.color.opacity(0.6), phase.color.opacity(0.2)],
                        center: .center,
                        startRadius: 0,
                        endRadius: 100
                    )
                )
                .frame(width: 200, height: 200)
                .scaleEffect(circleScale)
                .animation(.easeInOut(duration: phase.duration), value: circleScale)

            // Inner glow
            Circle()
                .fill(phase.color.opacity(0.3))
                .frame(width: 60, height: 60)
                .scaleEffect(circleScale)
                .blur(radius: 10)
                .animation(.easeInOut(duration: phase.duration), value: circleScale)
        }
    }

    // MARK: - Controls

    private var controlSection: some View {
        VStack(spacing: 16) {
            if !isRunning, roundCount == 0 || roundCount >= totalRounds {
                Stepper("Rounds: \(totalRounds)", value: $totalRounds, in: 1...20)
                    .frame(maxWidth: 200)
            }

            Button {
                if isRunning {
                    stop()
                } else {
                    start()
                }
            } label: {
                Label(
                    isRunning ? "Stop" : "Start",
                    systemImage: isRunning ? "stop.fill" : "play.fill"
                )
                .font(.headline)
                .frame(width: 140, height: 44)
            }
            .buttonStyle(.borderedProminent)
            .tint(isRunning ? .red : .blue)
            .controlSize(.large)
        }
    }

    // MARK: - Timer Control

    private func start() {
        roundCount = 0
        phase = .inhale
        phaseTimeRemaining = phase.duration
        isRunning = true
        circleScale = phase.scale

        timer = Timer.scheduledTimer(withTimeInterval: tickInterval, repeats: true) { _ in
            tick()
        }
    }

    private func stop() {
        timer?.invalidate()
        timer = nil
        isRunning = false
        withAnimation(.easeInOut(duration: 0.5)) {
            circleScale = 0.5
        }
    }

    private func tick() {
        phaseTimeRemaining -= tickInterval

        if phaseTimeRemaining <= 0 {
            advancePhase()
        }
    }

    private func advancePhase() {
        let nextPhase = phase.next

        // A full round completes when we cycle back to inhale
        if nextPhase == .inhale {
            roundCount += 1
            if roundCount >= totalRounds {
                stop()
                return
            }
        }

        phase = nextPhase
        phaseTimeRemaining = nextPhase.duration
        withAnimation(.easeInOut(duration: nextPhase.duration)) {
            circleScale = nextPhase.scale
        }
    }
}
