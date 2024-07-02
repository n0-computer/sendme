//
//  ProgressBarView.swift
//  sendme
//
//  Created by Brendan O'Brien on 3/28/24.
//

import SwiftUI

import SwiftUI

struct ProgressBarView: View {
    var progress: Float // Expected to be between 0.0 and 1.0
    
    var body: some View {
        GeometryReader { geometry in
            ZStack(alignment: .leading) {
                Rectangle().frame(width: geometry.size.width , height: geometry.size.height)
                    .opacity(0.3)
                    .foregroundColor(Color(UIColor.systemTeal))
                
                Rectangle().frame(width: min(CGFloat(self.progress)*geometry.size.width, geometry.size.width), height: geometry.size.height)
                    .foregroundColor(Color(UIColor.systemBlue))
                    .animation(.linear, value: progress)
            }.cornerRadius(45.0)
        }
    }
}
