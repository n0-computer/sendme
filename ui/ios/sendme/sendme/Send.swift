//
//  Send.swift
//  sendme
//
//  Created by Brendan O'Brien on 3/26/24.
//

import Foundation
import SwiftUI
import IrohLib
import UIKit
import CoreImage

enum SendStep {
  case choosing
  case sending
}

struct Send: View {
  @EnvironmentObject var nodeManager: IrohNodeManager
  @State private var step: SendStep = .choosing
  @State private var documentURLs: [URL] = []
  @State private var ticket: String = ""
  @State private var showingDocumentPicker: Bool = false
  @State private var showingShareSheet: Bool = false
  @State private var currentError: AppError?
  
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
    case .choosing:
      return AnyView(self.choose())
    case .sending:
      return AnyView(self.send())
    }
  }
  
  private func choose() -> some View {
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
      }.padding(EdgeInsets(top: 0, leading: 20, bottom: 20, trailing: 20))
      Spacer()
      Button("Choose Files to Send") {
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
              
              let res = try node.blobsCreateCollection(
                collection: collection,
                tag: SetTagOption.auto(),
                tagsToDelete: tagsToDelete
              )
              ticket = try node.blobsShare(
                  hash: res.hash,
                  blobFormat: BlobFormat.hashSeq,
                  ticketOptions: ShareTicketOptions.relay
              )
              print("generated collection ticket: \(ticket)")
              self.showingDocumentPicker = false
              
              // Delay the presentation of the share sheet to let self.ticket change propagate
              DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                self.step = .sending
              }
            } catch {
              self.currentError = .createCollectionFailed(error.localizedDescription)
            }
        }
    }
    .padding()
  }
  
  private func send() -> some View {
    return AnyView(VStack(spacing: 5){
      if let qrCode = generateQRCode(from: ticket) {
        ImageView(uiImage: qrCode)
      } else {
        Text("Image not found.")
      }
      HStack{
        Button("share") {
          self.showingShareSheet = true
        }
        Spacer()
        Button("done") {
          self.step = .choosing
          self.ticket = ""
        }
      }
    }.sheet(isPresented: $showingShareSheet) {
      ShareSheet(items: [self.ticket])
    }
    .padding())
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

func generateQRCode(from string: String) -> UIImage? {
    let data = string.data(using: String.Encoding.ascii)

    if let filter = CIFilter(name: "CIQRCodeGenerator") {
        filter.setValue(data, forKey: "inputMessage")
        filter.setValue("H", forKey: "inputCorrectionLevel")  // High correction level
        
        guard let outputCIImage = filter.outputImage else { return nil }
        
        // Scale the image
        let scaleX = 400 / outputCIImage.extent.width // The desired width
        let scaleY = 400 / outputCIImage.extent.height // The desired height
        let transformedImage = outputCIImage.transformed(by: CGAffineTransform(scaleX: scaleX, y: scaleY))
        
        // Convert to CGImage
        let context = CIContext()
        if let cgImage = context.createCGImage(transformedImage, from: transformedImage.extent) {
            return UIImage(cgImage: cgImage)
        }
    }

    return nil
}

#Preview {
  Send()
    .environmentObject(IrohNodeManager.shared)
    .onAppear() {
      IrohNodeManager.shared.start()
    }
}
