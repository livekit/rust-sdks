// Copyright 2026 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import SwiftUI

struct ContentView: View {
    var body: some View {
        switch AppEngine.shared.result {
        case .success(let engine):
            WakewordView(engine: engine)
        case .failure(let error):
            FailedView(message: error.localizedDescription)
        }
    }
}

private struct WakewordView: View {
    @ObservedObject var engine: WakewordEngine

    /// Rolling history buffers for the two graphs. They're plain reference
    /// objects so that `TimelineView` updates can append to them without
    /// forcing the whole view tree to re-render on every SwiftUI frame.
    @StateObject private var scoreHistory = TimeSeriesBuffer(capacity: 240)
    @StateObject private var volumeHistory = TimeSeriesBuffer(capacity: 240)

    var body: some View {
        VStack(spacing: 20) {
            Spacer(minLength: 8)

            header

            VStack(spacing: 16) {
                GraphPanel(
                    title: "Detection score",
                    value: engine.score,
                    displayValue: String(format: "%.3f", engine.score),
                    tint: engine.isTriggered ? .green : .blue,
                    threshold: 0.5
                ) {
                    TimeSeriesGraph(
                        buffer: scoreHistory,
                        range: 0...1,
                        threshold: 0.5,
                        tint: engine.isTriggered ? .green : .blue
                    )
                }

                GraphPanel(
                    title: "Mic level",
                    value: engine.volume,
                    displayValue: String(format: "%.0f%%", engine.volume * 100),
                    tint: .orange,
                    threshold: nil
                ) {
                    UVMeterGraph(buffer: volumeHistory)
                }
            }
            // Drive both graphs off a single shared tick that fires once per
            // audio buffer. This guarantees the detection and volume graphs
            // advance at identical cadences, so the detection line doesn't
            // look laggy/stuttery relative to the UV meter when `predict()`
            // runs at an irregular rate. The current `engine.score` is
            // sampled on each tick, which plots as a flat line between
            // predictions rather than skipping ahead unevenly.
            .onReceive(engine.$tick) { _ in
                scoreHistory.append(engine.score)
                volumeHistory.append(engine.volume)
            }

            if let err = engine.lastError {
                Text(err)
                    .font(.caption)
                    .foregroundStyle(.red)
                    .multilineTextAlignment(.center)
            }

            Button {
                engine.toggle()
            } label: {
                Label(
                    engine.isRunning ? "Mute mic" : "Unmute mic",
                    systemImage: engine.isRunning ? "mic.fill" : "mic.slash.fill"
                )
                .font(.title2.weight(.semibold))
                .frame(maxWidth: .infinity)
                .padding(.vertical, 12)
            }
            .buttonStyle(.borderedProminent)
            .tint(engine.isRunning ? .red : .blue)
            .controlSize(.large)

            Spacer(minLength: 8)
        }
        .padding(20)
    }

    private var header: some View {
        HStack(spacing: 10) {
            Circle()
                .fill(statusColor)
                .frame(width: 10, height: 10)
                .overlay(
                    Circle()
                        .stroke(statusColor.opacity(0.35), lineWidth: 4)
                        .scaleEffect(engine.isTriggered ? 2.0 : 1.0)
                        .opacity(engine.isTriggered ? 0 : 1)
                        .animation(.easeOut(duration: 0.8), value: engine.isTriggered)
                )
            Text(statusText)
                .font(.headline)
                .foregroundStyle(engine.isTriggered ? .green : .secondary)
            Spacer()
        }
    }

    private var statusColor: Color {
        if engine.isTriggered { return .green }
        return engine.isRunning ? .blue : .secondary
    }

    private var statusText: String {
        if engine.isTriggered { return "WAKE WORD DETECTED" }
        return engine.isRunning ? "Listening..." : "Mic muted"
    }
}

// MARK: - Time series storage

/// Fixed-capacity rolling window of `Float` samples. Uses `@Published` so
/// SwiftUI `Canvas` drawing closures re-run when new samples are appended.
@MainActor
final class TimeSeriesBuffer: ObservableObject {
    @Published private(set) var samples: [Float]
    let capacity: Int

    init(capacity: Int) {
        self.capacity = max(capacity, 2)
        self.samples = []
    }

    func append(_ value: Float) {
        if samples.count >= capacity {
            samples.removeFirst(samples.count - capacity + 1)
        }
        samples.append(value)
    }

    func reset() { samples.removeAll(keepingCapacity: true) }
}

// MARK: - Graph panel chrome

private struct GraphPanel<Content: View>: View {
    let title: String
    let value: Float
    let displayValue: String
    let tint: Color
    let threshold: Float?
    @ViewBuilder var content: () -> Content

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text(title)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(.secondary)
                Spacer()
                Text(displayValue)
                    .font(.system(.title3, design: .monospaced).weight(.semibold))
                    .foregroundStyle(tint)
                    .animation(.easeOut(duration: 0.15), value: value)
            }

            content()
                .frame(height: 140)
                .background(
                    RoundedRectangle(cornerRadius: 12)
                        .fill(Color.secondary.opacity(0.08))
                )
                .overlay(
                    RoundedRectangle(cornerRadius: 12)
                        .strokeBorder(Color.secondary.opacity(0.15), lineWidth: 1)
                )
        }
    }
}

// MARK: - Detection score graph

private struct TimeSeriesGraph: View {
    @ObservedObject var buffer: TimeSeriesBuffer
    let range: ClosedRange<Float>
    let threshold: Float?
    let tint: Color

    var body: some View {
        Canvas { ctx, size in
            drawGrid(ctx: ctx, size: size)
            if let threshold {
                drawThreshold(ctx: ctx, size: size, threshold: threshold)
            }
            drawLine(ctx: ctx, size: size)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
    }

    private func normalized(_ value: Float) -> CGFloat {
        let lo = range.lowerBound
        let hi = range.upperBound
        let clamped = min(max(value, lo), hi)
        return CGFloat((clamped - lo) / (hi - lo))
    }

    private func drawGrid(ctx: GraphicsContext, size: CGSize) {
        var grid = Path()
        let lines = 4
        for i in 0...lines {
            let y = size.height * CGFloat(i) / CGFloat(lines)
            grid.move(to: CGPoint(x: 0, y: y))
            grid.addLine(to: CGPoint(x: size.width, y: y))
        }
        ctx.stroke(
            grid,
            with: .color(.secondary.opacity(0.15)),
            style: StrokeStyle(lineWidth: 0.5, dash: [2, 3])
        )
    }

    private func drawThreshold(ctx: GraphicsContext, size: CGSize, threshold: Float) {
        let y = size.height * (1 - normalized(threshold))
        var path = Path()
        path.move(to: CGPoint(x: 0, y: y))
        path.addLine(to: CGPoint(x: size.width, y: y))
        ctx.stroke(
            path,
            with: .color(.green.opacity(0.6)),
            style: StrokeStyle(lineWidth: 1, dash: [4, 4])
        )
    }

    private func drawLine(ctx: GraphicsContext, size: CGSize) {
        let samples = buffer.samples
        guard samples.count > 1 else { return }
        let capacity = buffer.capacity
        let step = size.width / CGFloat(capacity - 1)
        let startIndex = capacity - samples.count

        var line = Path()
        for (i, s) in samples.enumerated() {
            let x = CGFloat(startIndex + i) * step
            let y = size.height * (1 - normalized(s))
            if i == 0 {
                line.move(to: CGPoint(x: x, y: y))
            } else {
                line.addLine(to: CGPoint(x: x, y: y))
            }
        }

        var fill = line
        let lastX = CGFloat(startIndex + samples.count - 1) * step
        let firstX = CGFloat(startIndex) * step
        fill.addLine(to: CGPoint(x: lastX, y: size.height))
        fill.addLine(to: CGPoint(x: firstX, y: size.height))
        fill.closeSubpath()

        ctx.fill(
            fill,
            with: .linearGradient(
                Gradient(colors: [tint.opacity(0.35), tint.opacity(0.02)]),
                startPoint: .zero,
                endPoint: CGPoint(x: 0, y: size.height)
            )
        )
        ctx.stroke(
            line,
            with: .color(tint),
            style: StrokeStyle(lineWidth: 2, lineCap: .round, lineJoin: .round)
        )
    }
}

// MARK: - UV meter (bar-style time series)

private struct UVMeterGraph: View {
    @ObservedObject var buffer: TimeSeriesBuffer

    var body: some View {
        Canvas { ctx, size in
            let samples = buffer.samples
            guard !samples.isEmpty else { return }

            let capacity = buffer.capacity
            let barSpacing: CGFloat = 1
            let totalWidth = size.width
            let barWidth = max(1, totalWidth / CGFloat(capacity) - barSpacing)
            let startIndex = capacity - samples.count

            for (i, s) in samples.enumerated() {
                let level = CGFloat(min(max(s, 0), 1))
                let h = level * size.height
                let x = CGFloat(startIndex + i) * (barWidth + barSpacing)
                let rect = CGRect(
                    x: x,
                    y: size.height - h,
                    width: barWidth,
                    height: h
                )
                let color = meterColor(for: level)
                ctx.fill(Path(rect), with: .color(color))
            }
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
    }

    private func meterColor(for level: CGFloat) -> Color {
        switch level {
        case ..<0.6: return .green
        case ..<0.85: return .yellow
        default: return .red
        }
    }
}

private struct FailedView: View {
    let message: String
    var body: some View {
        VStack(spacing: 16) {
            Text("Initialization failed")
                .font(.headline)
            Text(message)
                .font(.footnote)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
        }
        .padding()
    }
}

/// Holds the single `WakewordEngine` instance for the app's lifetime. Using a
/// static holder avoids `@StateObject` re-init and lets us expose a
/// `Result<WakewordEngine, Error>` without churning SwiftUI state.
@MainActor
final class AppEngine {
    static let shared = AppEngine()

    let result: Result<WakewordEngine, Error>

    private init() {
        do {
            result = .success(try WakewordEngine())
        } catch {
            result = .failure(error)
        }
    }
}

#Preview {
    ContentView()
}
