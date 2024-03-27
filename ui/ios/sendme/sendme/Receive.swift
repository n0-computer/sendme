//
//  Receive.swift
//  sendme
//
//  Created by Brendan O'Brien on 3/26/24.
//

import Foundation
import SwiftUI
import IrohLib
import CodeScanner

struct Receive: View {
  @EnvironmentObject var nodeManager: IrohNodeManager
  @State private var ticketString: String = ""
  @State private var showingQRScanner: Bool = false
  
  var scannerSheet: some View {
    CodeScannerView(
      codeTypes: [.qr],
      completion: { result in
        if case let .success(code) = result {
          self.ticketString = code.string
          self.showingQRScanner = false
        }
      })
  }
  
  var body: some View {
    VStack(spacing: 5) {
      VStack {
        Text("Receive")
          .font(Font.custom("Space Mono", size: 32))
          .foregroundColor(.primary)
          .frame(maxWidth: .infinity, alignment: .leading)
        Text("choose a document to get started")
          .font(Font.custom("Space Mono", size: 14))
          .foregroundColor(.secondary)
          .frame(maxWidth: .infinity, alignment: .leading)
      }.padding(EdgeInsets(top: 0, leading: 20, bottom: 10, trailing: 20))
      

      HStack{
        TextField("Paste Ticket", text: $ticketString, axis: .vertical)
          .textFieldStyle(.roundedBorder)
          .padding()

        Button("Download") {
          do {
            let node = self.nodeManager.node!
            let cb = DownloadProgressManager()
            try node.blobsDownloadTicket(ticket: ticketString, cb: cb)
          } catch {
            print("unknown error occurred")
          }
        }

        Button {
          showingQRScanner = true
        } label: {
          Image(systemName: "qrcode.viewfinder")
        }
        .sheet(isPresented: $showingQRScanner) {
          self.scannerSheet
        }
      }
    }
  }
}

class DownloadProgressManager: DownloadCallback {
  func progress(progress: DownloadProgress) throws {
    print("progress: \(progress)")
//    switch p.type() {
//    case .foundLocal:
//      <#code#>
//    case .connected:
//      <#code#>
//    case .found:
//      <#code#>
//    case .foundHashSeq:
//      <#code#>
//    case .progress:
//      <#code#>
//    case .done:
//      <#code#>
//    case .allDone:
//      <#code#>
//    case .abort:
//      <#code#>
//    }
  }
}

func saveFileToDocumentsDirectory(fileName: String, data: Data) {
    // Get the URL for the document directory
    let documentsDirectoryURL = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first!
    
    // Create the full file URL
    let fileURL = documentsDirectoryURL.appendingPathComponent(fileName)
    
    do {
        // Write the data to the file
        try data.write(to: fileURL)
        print("File saved: \(fileURL.absoluteString)")
    } catch {
        // Handle any errors
        print("Error saving file: \(error)")
    }
}

#Preview {
    Receive()
      .environmentObject(IrohNodeManager.shared)
      .onAppear() {
        IrohNodeManager.shared.start()
      }
}
