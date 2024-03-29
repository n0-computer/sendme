//
//  ContentView.swift
//  sendme
//
//  Created by Brendan O'Brien on 3/26/24.
//

import SwiftUI

struct ContentView: View {
    var body: some View {
      TabView {
        Receive()
          .tabItem {
            Label("Receive", systemImage: "macpro.gen2.fill")
          }
        Send()
          .tabItem {
            Label("Send", systemImage: "terminal")
          }
      }
    }
}

#Preview {
    ContentView()
      .environmentObject(IrohNodeManager.shared)
      .onAppear() {
        IrohNodeManager.shared.start()
      }
}
