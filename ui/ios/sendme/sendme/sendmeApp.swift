//
//  sendmeApp.swift
//  sendme
//
//  Created by Brendan O'Brien on 3/26/24.
//

import SwiftUI

@main
struct sendmeApp: App {
    @StateObject private var irohNodeManager = IrohNodeManager.shared
    var body: some Scene {
        WindowGroup {
            ContentView()
              .environmentObject(irohNodeManager)
              .onAppear() {
                  irohNodeManager.start()
              }
        }
    }
}
