import OSLog

public enum Log {
    static let subsystem = "com.themarchenko.shepit"
    public static let audio = Logger(subsystem: subsystem, category: "audio")
    public static let speech = Logger(subsystem: subsystem, category: "speech")
    public static let formatter = Logger(subsystem: subsystem, category: "formatter")
    public static let insert = Logger(subsystem: subsystem, category: "insert")
    public static let pipeline = Logger(subsystem: subsystem, category: "pipeline")
    public static let store = Logger(subsystem: subsystem, category: "store")
    public static let ui = Logger(subsystem: subsystem, category: "ui")
    public static let update = Logger(subsystem: subsystem, category: "update")
}
