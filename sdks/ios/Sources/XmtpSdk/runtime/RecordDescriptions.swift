// Generated from #[sdk(redact)] record fields. Do not edit this output.
import Foundation

extension StorageOptions: CustomStringConvertible, CustomDebugStringConvertible {
    public var description: String {
        return "StorageOptions(location: \(String(reflecting: self.`location`)), label: \(String(reflecting: self.`label`)), encryptionKey: <redacted>, pool: \(String(reflecting: self.`pool`)), singleConnection: \(String(reflecting: self.`singleConnection`)))"
    }

    public var debugDescription: String {
        description
    }
}

extension EncodedContent: CustomStringConvertible, CustomDebugStringConvertible {
    public var description: String {
        return "EncodedContent(type: \(String(reflecting: self.`type`)), parameters: \(String(reflecting: sdkRedacted(self.`parameters`, key: "secret"))), fallback: \(String(reflecting: self.`fallback`)), content: \(String(reflecting: self.`content`)))"
    }

    public var debugDescription: String {
        description
    }
}

extension RemoteAttachment: CustomStringConvertible, CustomDebugStringConvertible {
    public var description: String {
        return "RemoteAttachment(url: <redacted>, contentDigest: \(String(reflecting: self.`contentDigest`)), secret: <redacted>, salt: \(String(reflecting: self.`salt`)), nonce: \(String(reflecting: self.`nonce`)), scheme: \(String(reflecting: self.`scheme`)), contentLength: \(String(reflecting: self.`contentLength`)), filename: \(String(reflecting: self.`filename`)))"
    }

    public var debugDescription: String {
        description
    }
}

extension Credential: CustomStringConvertible, CustomDebugStringConvertible {
    public var description: String {
        return "Credential(name: \(String(reflecting: self.`name`)), value: <redacted>, expiresAtSeconds: \(String(reflecting: self.`expiresAtSeconds`)))"
    }

    public var debugDescription: String {
        description
    }
}

extension StreamBarrierTopic: CustomStringConvertible, CustomDebugStringConvertible {
    public var description: String {
        return "StreamBarrierTopic(topic: <redacted>, scopeGeneration: \(String(reflecting: self.`scopeGeneration`)), target: \(String(reflecting: self.`target`)), received: \(String(reflecting: self.`received`)), processed: \(String(reflecting: self.`processed`)), unresolvedWelcomes: \(String(reflecting: self.`unresolvedWelcomes`)), inactive: \(String(reflecting: self.`inactive`)), cause: \(String(reflecting: self.`cause`)))"
    }

    public var debugDescription: String {
        description
    }
}

extension AttachmentFailed: CustomStringConvertible, CustomDebugStringConvertible {
    public var description: String {
        return "AttachmentFailed(attachmentKey: \(String(reflecting: self.`attachmentKey`)), url: <redacted>, contentDigest: \(String(reflecting: self.`contentDigest`)), cause: \(String(reflecting: self.`cause`)))"
    }

    public var debugDescription: String {
        description
    }
}

extension AttachmentRef: CustomStringConvertible, CustomDebugStringConvertible {
    public var description: String {
        return "AttachmentRef(attachmentKey: \(String(reflecting: self.`attachmentKey`)), url: <redacted>, contentDigest: \(String(reflecting: self.`contentDigest`)))"
    }

    public var debugDescription: String {
        description
    }
}

extension PrepareMigrationArchiveArgs: CustomStringConvertible, CustomDebugStringConvertible {
    public var description: String {
        return "PrepareMigrationArchiveArgs(databasePath: \(String(reflecting: self.`databasePath`)), databaseKey: <redacted>, archiveKey: <redacted>, outputPath: \(String(reflecting: self.`outputPath`)))"
    }

    public var debugDescription: String {
        description
    }
}

extension HmacKey: CustomStringConvertible, CustomDebugStringConvertible {
    public var description: String {
        return "HmacKey(key: <redacted>, epoch: \(String(reflecting: self.`epoch`)))"
    }

    public var debugDescription: String {
        description
    }
}

extension NotificationChannel: CustomStringConvertible, CustomDebugStringConvertible {
    public var description: String {
        switch self {
        case .apns: return "NotificationChannel.apns(token: <redacted>)"
        case .fcm: return "NotificationChannel.fcm(token: <redacted>)"
        case .http: return "NotificationChannel.http(url: <redacted>, signingKey: <redacted>)"
        }
    }

    public var debugDescription: String {
        description
    }
}

private func sdkRedacted(_ map: [String: String], key: String) -> [String: String] {
    var map = map
    if map[key] != nil {
        map[key] = "<redacted>"
    }
    return map
}
