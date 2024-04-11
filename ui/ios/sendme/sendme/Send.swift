//
//  Send.swift
//  sendme
//
//  Created by Brendan O'Brien on 3/26/24.
//

import Foundation
import SwiftUI
import IrohLib

struct Send: View {
  @EnvironmentObject var nodeManager: IrohNodeManager
  @State private var documentURLs: [URL] = []
  @State private var ticket: String = ""
  @State private var showingDocumentPicker: Bool = false
  @State private var showingShareSheet: Bool = false
  
  var body: some View {
    VStack(spacing: 5) {
      VStack {
        Text("Send")
          .font(Font.custom("Space Mono", size: 32))
          .foregroundColor(.primary)
          .frame(maxWidth: .infinity, alignment: .leading)
        Text("share file(s)")
          .font(Font.custom("Space Mono", size: 14))
          .foregroundColor(.secondary)
          .frame(maxWidth: .infinity, alignment: .leading)
      }.padding(EdgeInsets(top: 0, leading: 20, bottom: 10, trailing: 20))
      Button("Pick a Document") {
        showingDocumentPicker = true
      }
    }
    .sheet(isPresented: $showingDocumentPicker) {
        DocumentPicker { urls in
            self.documentURLs = urls
          
            do {
              let node = self.nodeManager.node!
              
              let collection = Collection()
              var tagsToDelete: [String] = []
              for url in urls {
                let bytes = readFileContents(at: documentURLs.first!)
                let res = try node.blobsAddBytes(bytes: bytes!)
                
                try collection.push(name: url.lastPathComponent, hash: res.hash)
                
                if let tag = String(data: res.tag, encoding: .utf8) {
                  tagsToDelete.append(tag)
                }
              }
              print("created collection \(collection)")
              
              let res = try node.blobsCreateCollection(collection: collection, tag: SetTagOption.auto(), tagsToDelete: tagsToDelete)
              ticket = try node.blobsShare(hash: res.hash, blobFormat: BlobFormat.hashSeq, ticketOptions: ShareTicketOptions.relayAndAddresses)
              print("generated collection ticket: \(ticket)")
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
