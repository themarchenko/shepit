// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "ShepitCore",
    platforms: [.macOS(.v15)],
    products: [
        .library(name: "ShepitCore", targets: ["ShepitCore"]),
    ],
    dependencies: [
        .package(url: "https://github.com/FluidInference/FluidAudio.git", exact: "0.17.7"),
        .package(url: "https://github.com/argmaxinc/argmax-oss-swift.git", exact: "1.1.1"),
        .package(url: "https://github.com/groue/GRDB.swift.git", exact: "7.11.1"),
    ],
    targets: [
        .target(
            name: "ShepitCore",
            dependencies: [
                .product(name: "FluidAudio", package: "FluidAudio"),
                .product(name: "WhisperKit", package: "argmax-oss-swift"),
                .product(name: "GRDB", package: "GRDB.swift"),
            ],
            swiftSettings: [.swiftLanguageMode(.v5)]
        ),
        .executableTarget(
            name: "shepit-smoke",
            dependencies: ["ShepitCore"],
            swiftSettings: [.swiftLanguageMode(.v5)]
        ),
        .testTarget(
            name: "ShepitCoreTests",
            dependencies: ["ShepitCore"],
            swiftSettings: [.swiftLanguageMode(.v5)]
        ),
    ]
)
