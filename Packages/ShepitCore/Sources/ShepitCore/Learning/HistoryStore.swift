import Foundation
import GRDB

public struct DictationRecord: Codable, FetchableRecord, MutablePersistableRecord, Identifiable, Sendable {
    public static let databaseTableName = "dictations"

    public var id: Int64?
    public var createdAt: Date
    public var bundleID: String?
    public var appName: String?
    public var windowTitle: String?
    public var profile: String
    public var mode: String
    public var engine: String
    public var rawText: String
    public var formattedText: String
    public var finalText: String?
    public var language: String?
    public var audioSeconds: Double
    public var sttMs: Int
    public var llmMs: Int
    public var formatterSource: String
    public var insertMethod: String
    public var status: String
    /// Name of the WAV in the recordings directory, if the audio was kept.
    public var audioFile: String?

    public init(id: Int64? = nil, createdAt: Date = Date(), bundleID: String?, appName: String?, windowTitle: String?,
                profile: String, mode: String, engine: String, rawText: String, formattedText: String, finalText: String? = nil,
                language: String?, audioSeconds: Double, sttMs: Int, llmMs: Int, formatterSource: String, insertMethod: String, status: String,
                audioFile: String? = nil) {
        self.id = id
        self.createdAt = createdAt
        self.bundleID = bundleID
        self.appName = appName
        self.windowTitle = windowTitle
        self.profile = profile
        self.mode = mode
        self.engine = engine
        self.rawText = rawText
        self.formattedText = formattedText
        self.finalText = finalText
        self.language = language
        self.audioSeconds = audioSeconds
        self.sttMs = sttMs
        self.llmMs = llmMs
        self.formatterSource = formatterSource
        self.insertMethod = insertMethod
        self.status = status
        self.audioFile = audioFile
    }

    public mutating func didInsert(_ inserted: InsertionSuccess) {
        id = inserted.rowID
    }
}

/// SQLite store for dictations (and, later, dictionary and profiles).
public final class HistoryStore: Sendable {
    private let dbQueue: DatabaseQueue

    public init(url: URL) throws {
        dbQueue = try DatabaseQueue(path: url.path)
        try migrate()
    }

    private func migrate() throws {
        var migrator = DatabaseMigrator()
        migrator.registerMigration("v1") { db in
            try db.create(table: "dictations") { t in
                t.autoIncrementedPrimaryKey("id")
                t.column("createdAt", .datetime).notNull()
                t.column("bundleID", .text)
                t.column("appName", .text)
                t.column("windowTitle", .text)
                t.column("profile", .text).notNull()
                t.column("mode", .text).notNull()
                t.column("engine", .text).notNull()
                t.column("rawText", .text).notNull()
                t.column("formattedText", .text).notNull()
                t.column("finalText", .text)
                t.column("language", .text)
                t.column("audioSeconds", .double).notNull()
                t.column("sttMs", .integer).notNull()
                t.column("llmMs", .integer).notNull()
                t.column("formatterSource", .text).notNull()
                t.column("insertMethod", .text).notNull()
                t.column("status", .text).notNull()
            }
        }
        migrator.registerMigration("v2") { db in
            try db.alter(table: "dictations") { t in
                t.add(column: "audioFile", .text)
            }
        }
        try migrator.migrate(dbQueue)
    }

    @discardableResult
    public func save(_ record: DictationRecord) throws -> DictationRecord {
        var r = record
        try dbQueue.write { db in try r.insert(db) }
        return r
    }

    public func recent(limit: Int = 100) throws -> [DictationRecord] {
        try dbQueue.read { db in
            try DictationRecord.order(Column("createdAt").desc).limit(limit).fetchAll(db)
        }
    }

    public func updateFinalText(id: Int64, text: String) throws {
        try dbQueue.write { db in
            try db.execute(sql: "UPDATE dictations SET finalText = ? WHERE id = ?", arguments: [text, id])
        }
    }

    public func count() throws -> Int {
        try dbQueue.read { db in try DictationRecord.fetchCount(db) }
    }
}
