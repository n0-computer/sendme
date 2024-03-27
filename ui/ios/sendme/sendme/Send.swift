//
//  Send.swift
//  sendme
//
//  Created by Brendan O'Brien on 3/26/24.
//

import Foundation
import SwiftUI

struct Send: View {
  @EnvironmentObject var nodeManager: IrohNodeManager
  @State private var documentURLs: [URL] = []
  @State private var ticket: String = ""
  @State private var showingDocumentPicker: Bool = false
  @State private var showingShareSheet: Bool = false
  
  var body: some View {
    VStack {
      Button("Pick a Document") {
        showingDocumentPicker = true
      }
    }
    .sheet(isPresented: $showingDocumentPicker) {
        DocumentPicker { urls in
            self.documentURLs = urls
          
            do {
              let node = self.nodeManager.node!
              let bytes = readFileContents(at: documentURLs.first!)
              
              node.blobs
              let res = try node.blobsAddBytes(bytes: bytes!)
              ticket = try node.blobsShare(hash: res.hash, blobFormat: res.format)
              print("generated blob ticket: \(ticket)")
              self.showingDocumentPicker = false
              
              // Delay the presentation of the share sheet to let self.ticket change propagate
              DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                self.showingShareSheet = true
              }
            } catch {
              print("sharing blob failed")
            }
        }
    }
    .sheet(isPresented: $showingShareSheet) {
      ShareSheet(items: [self.ticket])
    }
    .padding()
  }
}

func readFileContents(at url: URL) -> Data? {
    do {
        // Attempt to read the file contents into a Data object
        let data = try Data(contentsOf: url)
        return data
    } catch {
        // If an error occurs, print it and return nil
        print("Error reading file: \(error)")
        return nil
    }
}


#Preview {
  Send()
    .environmentObject(IrohNodeManager.shared)
    .onAppear() {
      IrohNodeManager.shared.start()
    }
}
