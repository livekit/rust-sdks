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

import AVFoundation
import Foundation
import LiveKitUniFFI
import SwiftUI

/// Drives the microphone and runs wake-word inference on rolling 2-second windows.
///
/// Design:
/// - `AVAudioEngine` taps the input node on the real-time audio thread.
/// - Each callback converts `Float32` -> `Int16` and appends to a ring buffer
///   protected by an `NSLock`.
/// - Every `predictInterval` seconds (tracked inside the lock), a snapshot
///   of the full ring is dispatched to a background queue where
///   `detector.predict()` runs.
/// - Results are published back to the main actor for the UI.
///
/// Important: we never touch `AVAudioEngine.inputNode` until the user explicitly
/// starts listening and microphone authorization has been granted. Doing so
/// before TCC prompts (especially in a sandboxed macOS app) can crash in
/// `AVAudioIONodeImpl::AUI()`.
final class WakewordEngine: ObservableObject, @unchecked Sendable {

    // MARK: - Published UI state (always mutated on MainActor)

    @Published private(set) var isRunning = false
    @Published private(set) var score: Float = 0
    @Published private(set) var isTriggered = false
    @Published private(set) var lastError: String?
    /// Smoothed RMS level of the microphone input, normalized to 0...1.
    /// Updated on every audio buffer (roughly every ~90 ms by default).
    @Published private(set) var volume: Float = 0
    /// Monotonically increases once per audio-tap callback. Used by the UI
    /// as a shared time-axis signal so the detection and volume graphs
    /// advance at exactly the same rate, regardless of how often
    /// `predict()` actually completes.
    @Published private(set) var tick: UInt64 = 0

    // MARK: - Tuning

    /// Score at/above which we consider the wake word detected.
    private let triggerThreshold: Float = 0.5
    /// How long to keep the `isTriggered` indicator visible after a hit.
    private let triggerHoldDuration: TimeInterval = 1.5
    /// Minimum time between consecutive `predict()` runs. 20 ms gives us
    /// 50 Hz detection updates. If ONNX inference can't keep up on a given
    /// device, `predictInFlight` drops the excess ticks instead of queueing
    /// them so latency doesn't balloon.
    private let predictInterval: CFAbsoluteTime = 0.02
    /// Window length fed to the classifier (~2 s recommended by the crate).
    private let windowSeconds: Double = 2.0

    // MARK: - Internals

    /// The detector is created lazily (once the real hardware sample rate is
    /// known from `AVAudioEngine.inputNode`) and reused afterward.
    private var detector: WakeWordDetector?
    private let onnxURL: URL

    private var engine: AVAudioEngine?
    // Use .userInteractive so inference preempts other background work and
    // doesn't sit in runqueue limbo behind lower-priority tasks.
    private let workQueue = DispatchQueue(label: "io.livekit.wakeword.predict", qos: .userInteractive)

    /// Sample rate currently configured in `detector` and `ring`. Set on first
    /// successful `start()` and reused for subsequent starts if unchanged.
    private var configuredSampleRate: Double = 0

    /// Guards `ring`, `writeIdx`, `samplesWritten`, `lastPredictAt`,
    /// `predictInFlight`.
    private let ringLock = NSLock()
    private var ring: [Int16] = []
    private var writeIdx = 0
    private var samplesWritten = 0
    private var lastPredictAt: CFAbsoluteTime = 0
    /// True while a background `predict()` is running. Used to drop new
    /// requests rather than queuing them up when inference is slower than
    /// `predictInterval` on a given device.
    private var predictInFlight = false

    private var triggerResetTask: Task<Void, Never>?

    // MARK: - Init

    init() throws {
        guard let url = Bundle.main.url(forResource: "hey_livekit", withExtension: "onnx") else {
            throw NSError(
                domain: "WakewordEngine",
                code: 1,
                userInfo: [NSLocalizedDescriptionKey: "hey_livekit.onnx not found in app bundle"]
            )
        }
        self.onnxURL = url
    }

    // MARK: - Public API (called from UI / MainActor)

    @MainActor
    func toggle() {
        if isRunning {
            stop()
        } else {
            Task { await self.startAfterAuth() }
        }
    }

    // MARK: - Permission + start

    @MainActor
    private func startAfterAuth() async {
        let granted = await requestMicrophonePermission()
        guard granted else {
            lastError = "Microphone permission denied. Enable it in System Settings → Privacy & Security → Microphone."
            return
        }
        do {
            try start()
            lastError = nil
        } catch {
            lastError = "Start failed: \(error.localizedDescription)"
            stopInternal()
        }
    }

    private func requestMicrophonePermission() async -> Bool {
        if #available(iOS 17.0, macOS 14.0, *) {
            if AVAudioApplication.shared.recordPermission == .granted { return true }
            return await AVAudioApplication.requestRecordPermission()
        } else {
            #if os(iOS)
            let session = AVAudioSession.sharedInstance()
            if session.recordPermission == .granted { return true }
            return await withCheckedContinuation { cont in
                session.requestRecordPermission { cont.resume(returning: $0) }
            }
            #else
            // macOS pre-14 falls back to AVCaptureDevice.
            switch AVCaptureDevice.authorizationStatus(for: .audio) {
            case .authorized: return true
            case .notDetermined:
                return await withCheckedContinuation { cont in
                    AVCaptureDevice.requestAccess(for: .audio) { cont.resume(returning: $0) }
                }
            default: return false
            }
            #endif
        }
    }

    // MARK: - Start / Stop (MainActor)

    @MainActor
    private func start() throws {
        try configureAudioSession()

        let engine = AVAudioEngine()
        self.engine = engine

        let input = engine.inputNode
        let hwFormat = input.inputFormat(forBus: 0)

        guard hwFormat.sampleRate > 0 else {
            throw NSError(
                domain: "WakewordEngine",
                code: 2,
                userInfo: [NSLocalizedDescriptionKey: "Input has no valid sample rate (is a microphone connected?)"]
            )
        }

        // (Re)build the detector and ring buffer if the sample rate changed
        // (e.g. the user switched input devices between runs on macOS).
        if detector == nil || configuredSampleRate != hwFormat.sampleRate {
            detector = try WakeWordDetector(
                classifierPaths: [onnxURL.path],
                sampleRate: UInt32(hwFormat.sampleRate)
            )
            let ringSize = max(Int(hwFormat.sampleRate * windowSeconds), 1)
            ringLock.lock()
            ring = [Int16](repeating: 0, count: ringSize)
            writeIdx = 0
            samplesWritten = 0
            lastPredictAt = 0
            predictInFlight = false
            ringLock.unlock()
            configuredSampleRate = hwFormat.sampleRate
        } else {
            ringLock.lock()
            writeIdx = 0
            samplesWritten = 0
            lastPredictAt = 0
            predictInFlight = false
            ringLock.unlock()
        }

        guard let targetFormat = AVAudioFormat(
            commonFormat: .pcmFormatInt16,
            sampleRate: hwFormat.sampleRate,
            channels: 1,
            interleaved: true
        ) else {
            throw NSError(
                domain: "WakewordEngine",
                code: 3,
                userInfo: [NSLocalizedDescriptionKey: "Could not create target Int16 format"]
            )
        }
        guard let converter = AVAudioConverter(from: hwFormat, to: targetFormat) else {
            throw NSError(
                domain: "WakewordEngine",
                code: 4,
                userInfo: [NSLocalizedDescriptionKey: "Could not create AVAudioConverter"]
            )
        }

        // Small buffer size => shorter capture-side latency. At 48 kHz, 1024
        // frames is ~21 ms (vs ~85 ms for the old 4096). AVAudioEngine may
        // round this up to the HAL's preferred size, but it never gives us a
        // larger buffer than we asked for.
        input.installTap(onBus: 0, bufferSize: 1024, format: hwFormat) { [weak self] buffer, _ in
            self?.handleInput(buffer: buffer, converter: converter, targetFormat: targetFormat)
        }

        engine.prepare()
        try engine.start()
        isRunning = true
    }

    @MainActor
    private func stop() {
        stopInternal()
    }

    @MainActor
    private func stopInternal() {
        engine?.inputNode.removeTap(onBus: 0)
        engine?.stop()
        engine = nil

        #if os(iOS)
        try? AVAudioSession.sharedInstance().setActive(false, options: [.notifyOthersOnDeactivation])
        #endif

        ringLock.lock()
        samplesWritten = 0
        writeIdx = 0
        lastPredictAt = 0
        predictInFlight = false
        ringLock.unlock()

        isRunning = false
        score = 0
        volume = 0
        tick = 0
        isTriggered = false
        triggerResetTask?.cancel()
        triggerResetTask = nil
    }

    private func configureAudioSession() throws {
        #if os(iOS)
        let session = AVAudioSession.sharedInstance()
        try session.setCategory(.playAndRecord, mode: .measurement, options: [.defaultToSpeaker])
        try session.setActive(true, options: [])
        #endif
        // On macOS `AVAudioEngine` uses the current default input device directly;
        // there is no per-process audio session to configure.
    }

    // MARK: - Audio tap (real-time thread)

    private func handleInput(
        buffer inputBuffer: AVAudioPCMBuffer,
        converter: AVAudioConverter,
        targetFormat: AVAudioFormat
    ) {
        guard let outBuffer = AVAudioPCMBuffer(
            pcmFormat: targetFormat,
            frameCapacity: inputBuffer.frameCapacity
        ) else { return }

        var consumed = false
        var error: NSError?
        let status = converter.convert(to: outBuffer, error: &error) { _, outStatus in
            if consumed {
                outStatus.pointee = .noDataNow
                return nil
            }
            consumed = true
            outStatus.pointee = .haveData
            return inputBuffer
        }

        guard status != .error, error == nil,
              let channelData = outBuffer.int16ChannelData else {
            return
        }

        let frameCount = Int(outBuffer.frameLength)
        guard frameCount > 0 else { return }

        let level = computeLevel(samples: channelData[0], count: frameCount)
        Task { @MainActor [weak self] in
            self?.publish(volume: level)
        }

        let shouldRun = appendAndCheck(samples: channelData[0], count: frameCount)
        if shouldRun, let snapshot = snapshotRing() {
            workQueue.async { [weak self] in
                self?.runPredict(snapshot: snapshot)
            }
        }
    }

    /// Appends `count` samples and returns true if it's time to run predict.
    /// Called on audio thread; synchronized via `ringLock`.
    private func appendAndCheck(samples: UnsafePointer<Int16>, count: Int) -> Bool {
        ringLock.lock()
        defer { ringLock.unlock() }

        let size = ring.count
        guard size > 0 else { return false }
        var idx = writeIdx
        for i in 0..<count {
            ring[idx] = samples[i]
            idx += 1
            if idx >= size { idx = 0 }
        }
        writeIdx = idx
        samplesWritten = min(samplesWritten + count, size)

        guard samplesWritten >= size else { return false }
        let now = CFAbsoluteTimeGetCurrent()
        guard (now - lastPredictAt) >= predictInterval else { return false }
        // Drop this tick if the previous predict() is still running; queuing
        // them would increase latency instead of reducing it.
        guard !predictInFlight else { return false }
        lastPredictAt = now
        predictInFlight = true
        return true
    }

    /// Compute a normalized 0...1 loudness indicator from raw Int16 samples.
    ///
    /// We take the RMS in linear PCM units, convert to dBFS, then map a
    /// ~60 dB range (-60 dBFS silence .. 0 dBFS full-scale) to 0...1 so the
    /// UV meter reacts usefully to speech without pinning to the ceiling.
    private func computeLevel(samples: UnsafePointer<Int16>, count: Int) -> Float {
        guard count > 0 else { return 0 }
        var sumSquares: Double = 0
        for i in 0..<count {
            let v = Double(samples[i])
            sumSquares += v * v
        }
        let rms = sqrt(sumSquares / Double(count))
        let normalized = rms / Double(Int16.max)
        let floorDb: Double = -60
        let db: Double
        if normalized <= 1e-7 {
            db = floorDb
        } else {
            db = max(floorDb, 20.0 * log10(normalized))
        }
        let level = (db - floorDb) / -floorDb
        return Float(max(0, min(1, level)))
    }

    /// Take a linearized copy of the ring buffer in chronological order.
    private func snapshotRing() -> [Int16]? {
        ringLock.lock()
        defer { ringLock.unlock() }
        let size = ring.count
        guard samplesWritten >= size, size > 0 else { return nil }
        var out = [Int16](repeating: 0, count: size)
        let tail = size - writeIdx
        out.withUnsafeMutableBufferPointer { dst in
            ring.withUnsafeBufferPointer { src in
                guard let srcBase = src.baseAddress, let dstBase = dst.baseAddress else { return }
                dstBase.update(from: srcBase + writeIdx, count: tail)
                if writeIdx > 0 {
                    (dstBase + tail).update(from: srcBase, count: writeIdx)
                }
            }
        }
        return out
    }

    private func runPredict(snapshot: [Int16]) {
        defer {
            ringLock.lock()
            predictInFlight = false
            ringLock.unlock()
        }
        guard let detector else { return }
        do {
            let scores = try detector.predict(pcmI16: snapshot)
            let maxScore = scores.map(\.score).max() ?? 0
            Task { @MainActor [weak self] in
                self?.publish(score: maxScore)
            }
        } catch {
            Task { @MainActor [weak self] in
                self?.lastError = "predict failed: \(error.localizedDescription)"
            }
        }
    }

    // MARK: - UI publish (MainActor)

    @MainActor
    private func publish(volume newVolume: Float) {
        // Simple one-pole smoothing so the UV meter doesn't flicker.
        let alpha: Float = 0.35
        volume = (alpha * newVolume) + ((1 - alpha) * volume)
        // Bump the shared tick so the UI appends a sample to *both* time
        // series graphs at the same cadence.
        tick &+= 1
    }

    @MainActor
    private func publish(score newScore: Float) {
        score = newScore
        if newScore >= triggerThreshold {
            isTriggered = true
            triggerResetTask?.cancel()
            let hold = triggerHoldDuration
            triggerResetTask = Task { [weak self] in
                try? await Task.sleep(nanoseconds: UInt64(hold * 1_000_000_000))
                if !Task.isCancelled {
                    await MainActor.run { self?.isTriggered = false }
                }
            }
        }
    }
}
