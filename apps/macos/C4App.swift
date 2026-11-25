import C4UI
import SwiftUI

@main
struct C4App: App {
    @State private var model: C4Model

    init() {
        let model = C4Model(remembering: .standard)
        // `open -a rustc4 --args <path>...`, or a path on the command line.
        let urls = C4Model.urls(
            forArguments: Array(CommandLine.arguments.dropFirst()),
            environment: ProcessInfo.processInfo.environment)
        if urls.isEmpty {
            // Started with nothing to show: what was showing last time.
            model.reopen()
        } else {
            model.open(urls)
        }
        _model = State(initialValue: model)
    }

    var body: some Scene {
        Window("rustc4", id: "main") {
            C4View(model: model)
                .frame(minHeight: 420)
        }
        .defaultSize(width: 1280, height: 820)
        .commands {
            CommandGroup(replacing: .newItem) {
                Button(model.screen.controls.open) { C4View.chooseModel(for: model) }
                    .keyboardShortcut("o")
            }
        }
    }
}
