// swift-tools-version:5.5
import PackageDescription

// The Debug view offers “swift: demo” for this executable.
let package = Package(
    name: "demo",
    targets: [
        .executableTarget(name: "demo", path: "Sources/demo")
    ]
)
