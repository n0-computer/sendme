//
//  util.swift
//  sendme
//
//  Created by Brendan O'Brien on 3/28/24.
//

import Foundation


func humanizeBytes(_ bytes: Int64) -> String {
    let formatter = ByteCountFormatter()
    formatter.allowedUnits = [.useKB, .useMB, .useGB, .useTB, .usePB, .useEB, .useZB, .useYBOrHigher, .useBytes] // Customize based on your needs
    formatter.countStyle = .file
    formatter.includesUnit = true
    formatter.isAdaptive = true
    formatter.zeroPadsFractionDigits = true
    return formatter.string(fromByteCount: bytes)
}
