// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "MastFFI",
    platforms: [.macOS(.v26)],
    products: [
        .library(name: "CMast", targets: ["CMast"]),
    ],
    targets: [
        .systemLibrary(
            name: "CMast",
            path: "Sources/CMast"
        ),
    ]
)
