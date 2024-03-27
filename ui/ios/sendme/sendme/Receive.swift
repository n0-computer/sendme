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

enum ReceiveStep {
  case configuring
  case downloading
  case finished
}

struct Receive: View {
  @EnvironmentObject var nodeManager: IrohNodeManager
  @State private var step: ReceiveStep = .configuring
  @State private var currentError: AppError?
  @State private var ticketString: String = ""
  @State private var showingQRScanner: Bool = false
  private var progressManager: DownloadProgressManager = DownloadProgressManager()
  
  var body: some View {
    stepView
  }
  
  private var stepView: some View {
    switch step {
    case .configuring:
      return AnyView(self.configure())
    case .downloading:
      return AnyView(self.downloading())
    case .finished:
      return AnyView(self.finished())
    }
  }
  
  private func configure() -> any View {
    return AnyView(VStack(spacing: 5) {
      VStack {
        Text("Receive")
          .font(Font.custom("Space Mono", size: 32))
          .foregroundColor(.primary)
          .frame(maxWidth: .infinity, alignment: .leading)
        Text("download to your device")
          .font(Font.custom("Space Mono", size: 14))
          .foregroundColor(.secondary)
          .frame(maxWidth: .infinity, alignment: .leading)
      }.padding(EdgeInsets(top: 0, leading: 20, bottom: 10, trailing: 20))
      
      
      VStack{
        TextField("Paste Ticket", text: $ticketString, axis: .vertical)
          .textFieldStyle(.roundedBorder)
          .padding()
        
        Button("Download") {
          step = .downloading
          DispatchQueue.global(qos: .userInteractive).async {
            defer {
              step = .finished
            }
            
            do {
              let node = self.nodeManager.node!
              
              let ticket = try BlobTicket(ticket: ticketString)
              if ticket.format() == BlobFormat.raw {
                currentError = .invalidTicket("this is a 'raw' file ticket that links to data without a filename. sendme needs 'collection'-type tickets")
                return
              }
              
              try node.blobsDownload(req: ticket.asDownloadRequest(), cb: progressManager)
              let blobs = try node.blobsGetCollection(hash: ticket.hash()).blobs()
              for blob in blobs {
                let data = try node.blobsReadToBytes(hash: blob.link)
                saveFileToDocumentsDirectory(fileName: blob.name, data: data)
              }
            } catch let error {
              currentError = .downloadFailed(error.localizedDescription)
            }
          }
        }
        
        Button {
          showingQRScanner = true
        } label: {
          Image(systemName: "qrcode.viewfinder")
        }
        .sheet(isPresented: $showingQRScanner) {
          CodeScannerView(
            codeTypes: [.qr],
            completion: { result in
              if case let .success(code) = result {
                self.ticketString = code.string
                self.showingQRScanner = false
              }
            })
        }
        .alert(item: $currentError) { error in
          Alert(title: Text(error.title), message: Text(error.localizedDescription), dismissButton: .default(Text("OK")))
        }
      }
    })
  }
  
  private func downloading() -> any View {
    return AnyView(VStack {
      Text("Downloading")
    }.padding())
  }
  
  private func finished() -> any View {
    return AnyView(VStack{
      Text("Done!")
      Button("ok") {
        self.ticketString = ""
        step = .configuring
      }
    }.padding())
  }
}

class DownloadProgressManager: DownloadCallback {
  
  func progress(progress: DownloadProgress) throws {
    switch progress.type() {
//    case .foundLocal:
//      debugPrint("found local: \(progress.asFound())")
    case .found:
      debugPrint("found: \(progress.asFound())")
//    case .foundHashSeq:
//      debugPrint("found HashSeq: \(progress.asFound())")
    case .progress:
      debugPrint("progress: \(progress.asProgress())")
    case .done:
      debugPrint("found done: \(progress.asDone())")
    case .allDone:
      debugPrint("allDone: \(progress)")
    case .abort:
      debugPrint("abort: \(progress.asAbort())")
    default:
      debugPrint("unknown progress event: \(progress)")
    }
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
