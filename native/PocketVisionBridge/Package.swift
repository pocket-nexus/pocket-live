// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "PocketVisionBridge",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "pocket-vision-bridge", targets: ["PocketVisionBridge"]),
    ],
    targets: [
        .target(
            name: "CFrameShare",
            publicHeadersPath: "include"
        ),
        .executableTarget(
            name: "PocketVisionBridge",
            dependencies: ["CFrameShare"],
            swiftSettings: [.swiftLanguageMode(.v5)]
        ),
    ]
)
