// Sources/XMTPiOS/Logging.swift

import os.log
import Foundation

/// Logger namespace for XMTP SDK
public enum XMTPLogger {
    /// Main logger for general XMTP operations
    public static let main = Logger(subsystem: "org.xmtp.ios", category: "Main")
    
    /// Database-specific logger
    public static let database = Logger(subsystem: "org.xmtp.ios", category: "Database")
    
}