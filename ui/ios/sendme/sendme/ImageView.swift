//
//  ImageView.swift
//  sendme
//
//  Created by Brendan O'Brien on 3/28/24.
//

import SwiftUI

struct ImageView: View {
  let uiImage: UIImage
  
  var body: some View {
    Image(uiImage: uiImage)
      .resizable()
      .scaledToFit()
  }
}
