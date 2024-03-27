//
//  IrohNodeManager.swift
//  iroh
//
//  Created by Brendan O'Brien on 8/8/23.
//

import SwiftUI
import IrohLib
import Foundation

class IrohNodeManager: ObservableObject {
  static let shared = IrohNodeManager()

  @Published var node: IrohNode?
  @Published var nodeID: String = ""
  @Published var author: AuthorId?
  @Published var nodeStats: [String : CounterStats]?
  @Published var connections: [ConnectionInfoIdentifiable]?
  @Published var connectionHistories: [String: [ConnHistory]] = [:]

  
  private var timer: Timer?
  
  func start() {
//    IrohLib.setLogLevel(level: .debug)
    do {
      try IrohLib.startMetricsCollection()
      let path = self.irohPath()
      print(path.absoluteString)
      self.node = try IrohNode(path: path.path)
      nodeID = node?.nodeId() ?? ""
      startConnectionMonitoring()
      startStatsMonitoring()
      initAuthor()
      print("created iroh node with node Id \(nodeID)")
    } catch {
      print("error creating iroh node \(error)")
    }
  }
  
  // TODO: using a single author for now. At some point we should add support for author selection
  private func initAuthor() {
    do {
      self.author = try self.node?.authorCreate()
    } catch {
      print("couldn't create author \(error)")
      return
    }
  }
  
  
  private func startConnectionMonitoring() {
    updateConnections()
    DispatchQueue.main.asyncAfter(deadline: .now() + 1.0) {
      self.startConnectionMonitoring()
    }
  }
  
  private func updateConnections() {
    guard let node = node else {
      print("Error: no node")
      return
    }

    do {
      let connections = try node.connections()

      DispatchQueue.global(qos: .userInteractive).async {
        let mapped = connections.map { (conn) -> ConnectionInfoIdentifiable in
          let nodeId = conn.nodeId.toString()
          let item = ConnHistory(
            id: .now,
            latency: conn.latency != nil ? conn.latency! * 1000 : 0,
            connType: conn.connType)

          DispatchQueue.main.async {
            if self.connectionHistories[nodeId] != nil {
              self.connectionHistories[nodeId]?.append(item)
            } else {
              self.connectionHistories[nodeId] = [item]
            }

            if self.connectionHistories[nodeId]?.count ?? 0 >= 300 {
              let _ = self.connectionHistories[nodeId]!.popLast()
            }
          }

          return ConnectionInfoIdentifiable(
            id: nodeId,
            latency: conn.latency != nil ? conn.latency! * 1000 : nil,
            connType: conn.connType)
        }
        
        DispatchQueue.main.async {
          self.connections = mapped
        }
      }
    } catch {
      print("error fetching connections")
    }
  }
  
  private func startStatsMonitoring() {
    timer = Timer.scheduledTimer(withTimeInterval: 3, repeats: true) { _ in
      do {
        if let latest = try self.node?.stats() {
          
          DispatchQueue.main.async {
            self.nodeStats = latest
          }

        }
      } catch (let error) {
        print("error \(error)")
        self.timer?.invalidate()
        self.timer = nil
      }
    }
  }
  
  private func irohPath() -> URL {
    let paths = FileManager.default.urls(for: .libraryDirectory, in: .userDomainMask)
    let irohPath = paths[0].appendingPathComponent("iroh")
    mkdirP(path: irohPath.path)
    return irohPath
  }
}

struct IrohNodeManagerEnvironmentKey: EnvironmentKey {
    static var defaultValue: IrohNodeManager = IrohNodeManager.shared
}

extension EnvironmentValues {
    var irohNodeManager: IrohNodeManager {
        get { self[IrohNodeManagerEnvironmentKey.self] }
        set { self[IrohNodeManagerEnvironmentKey.self] = newValue }
    }
}

struct ConnectionInfoIdentifiable: Identifiable, Equatable {
  var id: String
  var latency: Double?
  var connType: ConnectionType

  static func == (lhs: ConnectionInfoIdentifiable, rhs: ConnectionInfoIdentifiable) -> Bool {
    lhs.id == rhs.id
  }

  func latencyString() -> String {
    if let latency = latency {
      return String(format:"%.1f", latency)
    }
    return "?"
  }
}

struct ConnHistory: Identifiable {
  var id: Date
  var latency: Double
  var connType: ConnectionType
}

func mkdirP(path: String) {
    let fileManager = FileManager.default
    
    do {
        try fileManager.createDirectory(atPath: path,
                                        withIntermediateDirectories: true,
                                        attributes: nil)
    } catch {
        print("Error creating directory: \(error)")
    }
}
