//
//  AppError.swift
//  sendme
//
//  Created by Brendan O'Brien on 3/27/24.
//

import Foundation


enum AppError: Error, Identifiable {
    var id: String { localizedDescription }
    
    case invalidTicket(String)
    case downloadFailed(String)
    case createCollectionFailed(String)
}

extension AppError: LocalizedError {
    var errorDescription: String? {
        switch self {
        case .invalidTicket(let error):
          return NSLocalizedString(error, comment: "")
        case .downloadFailed(let error):
          return NSLocalizedString(error, comment: "")
        case .createCollectionFailed(let error):
          return NSLocalizedString(error, comment: "")
        }
    }
}

extension AppError {
  var title: String {
    switch self {
    case .invalidTicket(_):
      return NSLocalizedString("Invalid Ticket", comment: "")
    case .downloadFailed(_):
      return NSLocalizedString("Download Failed", comment: "")
    case .createCollectionFailed(_):
      return NSLocalizedString("Import Failed", comment: "")
    }
  }
}
