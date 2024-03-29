//
//  Receive.swift
//  sendme
//
//  Created by Brendan O'Brien on 3/26/24.
//

import Foundation
import SwiftUI
import UIKit
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
    VStack {
      stepView
    }
    .padding(EdgeInsets(top: 20, leading: 0, bottom: 20, trailing: 0))
    .alert(item: $currentError) { error in
      Alert(title: Text(error.title), message: Text(error.localizedDescription), dismissButton: .default(Text("OK")))
    }
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
      HStack {
        VStack {
          Text("Receive")
            .font(Font.custom("Space Mono", size: 32))
            .foregroundColor(.primary)
            .frame(maxWidth: .infinity, alignment: .leading)
          Text("download to your device")
            .font(Font.custom("Space Mono", size: 14))
            .foregroundColor(.secondary)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        Button {
          showingQRScanner = true
        } label: {
          Image(systemName: "qrcode.viewfinder")
            .scaleEffect(CGSize(width: 1.5, height: 1.5))
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
      }
      .padding(EdgeInsets(top: 0, leading: 16, bottom: 10, trailing: 20))
      VStack{
        ZStack(alignment: .topLeading) {
            TextEditor(text: $ticketString)
                .padding()
                .overlay(
                    RoundedRectangle(cornerRadius: 5) // Use RoundedRectangle for rounded corners
                        .stroke(Color.gray, lineWidth: 1) // Border color and width
                )
                .clipShape(RoundedRectangle(cornerRadius: 5)) // Clip the shape with rounded corners
            if ticketString.isEmpty {
                Text("Paste ticket here, or scan QR code")
                    .foregroundColor(.gray)
                    .padding(24) // Match the padding of the TextEditor
            }
        }
        .frame(height: 200)
        .padding()
        
        Spacer()
        
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
              
              try node.blobsDownload(
                req: ticket.asDownloadRequest(),
                cb: progressManager
              )
              let documentsDirectoryURL = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first!
              print("exporting to: \(documentsDirectoryURL.relativePath)")
              try node.blobsExport(
                hash: ticket.hash(),
                destination: documentsDirectoryURL.relativePath,
                format: BlobExportFormat.collection,
                mode: BlobExportMode.tryReference
              )
            } catch let error {
              currentError = .downloadFailed(error.localizedDescription)
            }
          }
        }
      }
    })
  }
  
  private func downloading() -> any View {
    return AnyView(VStack(spacing: 5) {
      ProgressBarView(progress: self.progressManager.completion())
        .frame(height: 14)
        .padding()
      Text("Downloading")
    }.padding())
  }
  
  private func finished() -> any View {
    return AnyView(VStack{
      HStack {
        VStack {
          Text("Done!")
          if let res = progressManager.result {
            Text("Downloaded \(progressManager.totalChildren) files totalling \(humanizeBytes(Int64(res.bytesRead))) in \(res.elapsed)")
          }
        }
        Button("ok") {
          progressManager.reset()
          self.ticketString = ""
          step = .configuring
        }
      }
      Spacer()
      Button("Show in Files App") {
          if let url = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first {
              // Switch URL scheme to this psudo-documents "shareddocuments":
              let url = URL(string: "shareddocuments://" + url.path())!
              print("opening \(url)")
              UIApplication.shared.open(url)
          }
      }
    }.padding())
  }
}

class DownloadProgressManager: DownloadCallback {
  var totalChildren: UInt64 = 0
  private var fetchedChildren: UInt64 = 0
  var result: DownloadProgressAllDone?
  
  // how far along download progress is
  func completion() -> Float {
    if totalChildren == 0 {
      return 0.0
    }
    return Float(fetchedChildren) / Float(totalChildren)
  }
  
  
  func reset() {
    totalChildren = 0
    fetchedChildren = 0
    result = nil
  }
  
  func progress(progress: DownloadProgress) throws {
    switch progress.type() {
    case .foundLocal:
      debugPrint("found local: \(progress.asFoundLocal())")
      fetchedChildren += 1
    case .found:
      debugPrint("found: \(progress.asFound())")
    case .foundHashSeq:
      debugPrint("found HashSeq: \(progress.asFoundHashSeq())")
      totalChildren = progress.asFoundHashSeq().children
    case .progress:
      debugPrint("progress: \(progress.asProgress())")
    case .done:
      debugPrint("found done: \(progress.asDone())")
      fetchedChildren += 1
    case .allDone:
      debugPrint("allDone: \(progress)")
      fetchedChildren = totalChildren
      result = progress.asAllDone()
    case .abort:
      debugPrint("abort: \(progress.asAbort())")
    default:
      debugPrint("unknown progress event: \(progress)")
    }
  }
}

#Preview {
    Receive()
      .environmentObject(IrohNodeManager.shared)
      .onAppear() {
        IrohNodeManager.shared.start()
      }
}
