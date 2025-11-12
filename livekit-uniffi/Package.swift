// swift-tools-version:6.0
// (Xcode16.0+)

// For local testing, run ./swift.sh to generate the xcframework

import PackageDescription

let package = Package(
    name: "LiveKitFFI",
    platforms: [
        .iOS(.v13),
        .macOS(.v10_15),
        .macCatalyst(.v14),
        .visionOS(.v2),
        .tvOS(.v17),
    ],
    products: [
        .library(
            name: "LiveKitFFI",
            targets: ["LiveKitFFI"]
        )
    ],
    targets: [
        .binaryTarget(
            name: "LiveKitFFIBinary",
            path: "../target/LiveKitFFI.xcframework"
        ),
        .target(
            name: "LiveKitFFI",
            dependencies: ["LiveKitFFIBinary"],
            path: "generated/swift",
            sources: ["livekit_uniffi.swift"],
            publicHeadersPath: "."
        )
    ]
)
