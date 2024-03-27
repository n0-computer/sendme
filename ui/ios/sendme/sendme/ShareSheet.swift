//
//  ShareSheet.swift
//  sendme
//
//  Created by Brendan O'Brien on 3/26/24.
//

import SwiftUI
import UIKit

struct ShareSheet: UIViewControllerRepresentable {
    var items: [Any]
    
    func makeUIViewController(context: Context) -> UIActivityViewController {
        print("sharing items \(items)")
        let activityViewController = UIActivityViewController(activityItems: items, applicationActivities: nil)
        return activityViewController
    }
    
    func updateUIViewController(_ uiViewController: UIActivityViewController, context: Context) {
        // No need to update the view controller in this case.
    }
}

#Preview {
  ShareSheet(items: ["Hello, World!"])
}

