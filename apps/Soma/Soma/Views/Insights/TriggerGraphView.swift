import SwiftUI

// MARK: - Graph Node

private struct GraphNode: Identifiable {
    let id: String
    let label: String
    let kind: NodeKind
    var position: CGPoint
    var velocity: CGPoint = .zero

    enum NodeKind {
        case trigger
        case emotion

        var color: Color {
            switch self {
            case .trigger: .orange
            case .emotion: .blue
            }
        }
    }
}

// MARK: - Graph Edge Reference

private struct GraphEdgeRef: Identifiable {
    let id: String
    let sourceId: String
    let targetId: String
    let relation: String
    let weight: Float
}

// MARK: - Trigger Graph View

struct TriggerGraphView: View {
    let edges: [MastEdge]

    @State private var nodes: [GraphNode] = []
    @State private var edgeRefs: [GraphEdgeRef] = []
    @State private var timer: Timer?
    @State private var draggedNodeId: String?
    @State private var canvasSize: CGSize = .init(width: 600, height: 400)
    @State private var isSimulating = true
    @State private var tickCount = 0

    /// Maximum simulation ticks before auto-stopping to save CPU.
    private let maxTicks = 300

    var body: some View {
        if edges.isEmpty {
            ContentUnavailableView {
                Label("No Trigger Data", systemImage: "point.3.connected.trianglepath.dotted")
            } description: {
                Text("Trigger-emotion relationships appear here as you write journal entries and the AI extracts patterns.")
            }
        } else {
            graphCanvas
                .onAppear { buildGraph() }
                .onDisappear { stopSimulation() }
        }
    }

    // MARK: - Canvas

    private var graphCanvas: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Trigger-Emotion Graph")
                    .font(.headline)

                Spacer()

                legendItem(color: .orange, label: "Triggers")
                legendItem(color: .blue, label: "Emotions")

                Spacer()

                Button {
                    isSimulating.toggle()
                    if isSimulating {
                        tickCount = 0
                        startSimulation()
                    } else {
                        stopSimulation()
                    }
                } label: {
                    Label(
                        isSimulating ? "Pause" : "Resume",
                        systemImage: isSimulating ? "pause.fill" : "play.fill"
                    )
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
            }
            .padding()

            GeometryReader { geometry in
                let size = geometry.size

                Canvas { context, canvasSize in
                    // Draw edges
                    for edge in edgeRefs {
                        guard let source = nodes.first(where: { $0.id == edge.sourceId }),
                              let target = nodes.first(where: { $0.id == edge.targetId })
                        else { continue }

                        var path = Path()
                        path.move(to: source.position)
                        path.addLine(to: target.position)

                        let lineWidth = max(0.5, Double(edge.weight) * 3)
                        context.stroke(
                            path,
                            with: .color(.secondary.opacity(0.4)),
                            lineWidth: lineWidth
                        )
                    }

                    // Draw nodes
                    for node in nodes {
                        let radius: CGFloat = 18
                        let rect = CGRect(
                            x: node.position.x - radius,
                            y: node.position.y - radius,
                            width: radius * 2,
                            height: radius * 2
                        )

                        // Circle
                        let circle = Path(ellipseIn: rect)
                        context.fill(circle, with: .color(node.kind.color.opacity(0.8)))
                        context.stroke(circle, with: .color(node.kind.color), lineWidth: 1.5)

                        // Label
                        let text = Text(node.label)
                            .font(.system(size: 10, weight: .medium))
                            .foregroundStyle(.primary)
                        let labelPoint = CGPoint(x: node.position.x, y: node.position.y + radius + 10)
                        context.draw(
                            context.resolve(text),
                            at: labelPoint,
                            anchor: .top
                        )
                    }
                }
                .gesture(
                    DragGesture(minimumDistance: 0)
                        .onChanged { value in
                            if draggedNodeId == nil {
                                // Find the closest node within 20pt
                                draggedNodeId = nodes
                                    .min(by: {
                                        distance($0.position, value.startLocation) <
                                        distance($1.position, value.startLocation)
                                    })
                                    .flatMap {
                                        distance($0.position, value.startLocation) < 25 ? $0.id : nil
                                    }
                            }
                            if let id = draggedNodeId,
                               let index = nodes.firstIndex(where: { $0.id == id }) {
                                nodes[index].position = value.location
                                nodes[index].velocity = .zero
                            }
                        }
                        .onEnded { _ in
                            draggedNodeId = nil
                        }
                )
                .onChange(of: size) { _, newSize in
                    canvasSize = newSize
                }
                .onAppear {
                    canvasSize = size
                }
            }
        }
    }

    private func legendItem(color: Color, label: String) -> some View {
        HStack(spacing: 4) {
            Circle()
                .fill(color)
                .frame(width: 10, height: 10)
            Text(label)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    // MARK: - Graph Construction

    private func buildGraph() {
        // Collect unique entities
        var triggers = Set<String>()
        var emotions = Set<String>()

        for edge in edges {
            if edge.relation == "triggers" {
                triggers.insert(edge.source)
                emotions.insert(edge.target)
            } else {
                triggers.insert(edge.source)
                triggers.insert(edge.target)
            }
        }

        // Create nodes with random initial positions
        var builtNodes: [GraphNode] = []
        let center = CGPoint(x: canvasSize.width / 2, y: canvasSize.height / 2)
        let spread: CGFloat = min(canvasSize.width, canvasSize.height) * 0.35

        for trigger in triggers {
            let angle = Double.random(in: 0...(2 * .pi))
            let radius = Double.random(in: 0...spread)
            builtNodes.append(GraphNode(
                id: "t_\(trigger)",
                label: trigger,
                kind: .trigger,
                position: CGPoint(
                    x: center.x + cos(angle) * radius,
                    y: center.y + sin(angle) * radius
                )
            ))
        }

        for emotion in emotions {
            let angle = Double.random(in: 0...(2 * .pi))
            let radius = Double.random(in: 0...spread)
            builtNodes.append(GraphNode(
                id: "e_\(emotion)",
                label: emotion,
                kind: .emotion,
                position: CGPoint(
                    x: center.x + cos(angle) * radius,
                    y: center.y + sin(angle) * radius
                )
            ))
        }

        // Build edge references
        var builtEdges: [GraphEdgeRef] = []
        for edge in edges {
            let sourceId = edge.relation == "triggers" ? "t_\(edge.source)" : "t_\(edge.source)"
            let targetId = edge.relation == "triggers" ? "e_\(edge.target)" : "t_\(edge.target)"
            builtEdges.append(GraphEdgeRef(
                id: edge.id,
                sourceId: sourceId,
                targetId: targetId,
                relation: edge.relation,
                weight: edge.weight
            ))
        }

        nodes = builtNodes
        edgeRefs = builtEdges
        tickCount = 0
        startSimulation()
    }

    // MARK: - Force Simulation

    private func startSimulation() {
        stopSimulation()
        timer = Timer.scheduledTimer(withTimeInterval: 1.0 / 60.0, repeats: true) { _ in
            simulationTick()
        }
    }

    private func stopSimulation() {
        timer?.invalidate()
        timer = nil
    }

    private func simulationTick() {
        guard nodes.count > 1 else { return }

        tickCount += 1
        if tickCount >= maxTicks {
            isSimulating = false
            stopSimulation()
            return
        }

        let coolingFactor = max(0.1, 1.0 - Double(tickCount) / Double(maxTicks))
        let repulsionStrength: CGFloat = 5000 * coolingFactor
        let attractionStrength: CGFloat = 0.005 * coolingFactor
        let damping: CGFloat = 0.85
        let centerPull: CGFloat = 0.001 * coolingFactor
        let center = CGPoint(x: canvasSize.width / 2, y: canvasSize.height / 2)
        let padding: CGFloat = 30

        var forces = Array(repeating: CGPoint.zero, count: nodes.count)

        // Repulsion between all pairs
        for i in 0..<nodes.count {
            for j in (i + 1)..<nodes.count {
                let dx = nodes[i].position.x - nodes[j].position.x
                let dy = nodes[i].position.y - nodes[j].position.y
                let dist = max(1, sqrt(dx * dx + dy * dy))
                let force = repulsionStrength / (dist * dist)
                let fx = (dx / dist) * force
                let fy = (dy / dist) * force
                forces[i].x += fx
                forces[i].y += fy
                forces[j].x -= fx
                forces[j].y -= fy
            }
        }

        // Attraction along edges
        for edge in edgeRefs {
            guard let si = nodes.firstIndex(where: { $0.id == edge.sourceId }),
                  let ti = nodes.firstIndex(where: { $0.id == edge.targetId })
            else { continue }

            let dx = nodes[ti].position.x - nodes[si].position.x
            let dy = nodes[ti].position.y - nodes[si].position.y
            let dist = max(1, sqrt(dx * dx + dy * dy))
            let idealLength: CGFloat = 120
            let displacement = dist - idealLength
            let fx = (dx / dist) * displacement * attractionStrength
            let fy = (dy / dist) * displacement * attractionStrength
            forces[si].x += fx
            forces[si].y += fy
            forces[ti].x -= fx
            forces[ti].y -= fy
        }

        // Gentle pull toward center
        for i in 0..<nodes.count {
            forces[i].x += (center.x - nodes[i].position.x) * centerPull
            forces[i].y += (center.y - nodes[i].position.y) * centerPull
        }

        // Apply forces
        for i in 0..<nodes.count {
            if nodes[i].id == draggedNodeId { continue }

            nodes[i].velocity.x = (nodes[i].velocity.x + forces[i].x) * damping
            nodes[i].velocity.y = (nodes[i].velocity.y + forces[i].y) * damping

            // Clamp velocity
            let maxVel: CGFloat = 10
            nodes[i].velocity.x = max(-maxVel, min(maxVel, nodes[i].velocity.x))
            nodes[i].velocity.y = max(-maxVel, min(maxVel, nodes[i].velocity.y))

            nodes[i].position.x += nodes[i].velocity.x
            nodes[i].position.y += nodes[i].velocity.y

            // Keep within bounds
            nodes[i].position.x = max(padding, min(canvasSize.width - padding, nodes[i].position.x))
            nodes[i].position.y = max(padding, min(canvasSize.height - padding, nodes[i].position.y))
        }
    }

    private func distance(_ a: CGPoint, _ b: CGPoint) -> CGFloat {
        let dx = a.x - b.x
        let dy = a.y - b.y
        return sqrt(dx * dx + dy * dy)
    }
}
