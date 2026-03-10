// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "Notes",
    platforms: [.iOS(.v17)],
    targets: [
        .executableTarget(
            name: "Notes",
            path: "Sources/Notes"
        )
    ]
)
