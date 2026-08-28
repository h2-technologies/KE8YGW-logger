import Foundation

enum LogExportService {
    static func adif(for qsos: [QSO]) -> String {
        var output = """
        KE8YGW Logger ADIF Export
        <ADIF_VER:5>3.1.4
        <PROGRAMID:12>KE8YGWLogger
        <PROGRAMVERSION:5>0.3.0
        <EOH>

        """

        for qso in qsos.sorted(by: { $0.contactDate < $1.contactDate }) {
            output += adifField("CALL", qso.callsign)
            output += adifField("QSO_DATE", HamDateFormatters.adifDate.string(from: qso.contactDate))
            output += adifField("TIME_ON", HamDateFormatters.adifTime.string(from: qso.contactDate))
            output += adifField("BAND", qso.band)
            output += adifField("MODE", qso.mode)
            output += adifField("SUBMODE", qso.submode)
            if qso.frequencyMHz > 0 {
                output += adifField("FREQ", String(format: "%.6f", qso.frequencyMHz))
            }
            output += adifField("RST_SENT", qso.rstSent)
            output += adifField("RST_RCVD", qso.rstReceived)
            if qso.powerWatts > 0 {
                output += adifField("TX_PWR", String(format: "%.0f", qso.powerWatts))
            }
            output += adifField("STATION_CALLSIGN", qso.stationCallsign)
            output += adifField("OPERATOR", qso.operatorCallsign)
            output += adifField("GRIDSQUARE", qso.gridSquare)
            output += adifField("NAME", qso.name)
            output += adifField("QTH", qso.qth)
            output += adifField("CNTY", qso.county)
            output += adifField("STATE", qso.state)
            output += adifField("COUNTRY", qso.country)
            output += adifField("SRX_STRING", qso.contestExchange)
            output += adifField("SAT_NAME", qso.satelliteName)
            output += adifField("MY_SIG", portableSignalName(for: qso))
            output += adifField("MY_SIG_INFO", portableSignalReference(for: qso))
            output += adifField("COMMENT", qso.notes)
            output += "<EOR>\n"
        }

        return output
    }

    static func csv(for qsos: [QSO]) -> String {
        let header = [
            "Callsign", "DateTimeUTC", "Type", "Band", "Mode", "Submode", "FrequencyMHz",
            "RSTSent", "RSTReceived", "PowerWatts", "Operator", "Station", "Equipment",
            "Grid", "County", "Name", "QTH", "State", "Country", "ContestExchange",
            "Satellite", "POTA", "SOTA", "UploadStatus", "SyncStatus", "Notes"
        ].joined(separator: ",")

        let rows = qsos.sorted(by: { $0.contactDate < $1.contactDate }).map { qso in
            [
                qso.callsign,
                HamDateFormatters.csvDateTime.string(from: qso.contactDate),
                qso.qsoKind,
                qso.band,
                qso.mode,
                qso.submode,
                String(format: "%.6f", qso.frequencyMHz),
                qso.rstSent,
                qso.rstReceived,
                String(format: "%.0f", qso.powerWatts),
                qso.operatorCallsign,
                qso.stationCallsign,
                qso.equipmentSummary,
                qso.gridSquare,
                qso.county,
                qso.name,
                qso.qth,
                qso.state,
                qso.country,
                qso.contestExchange,
                qso.satelliteName,
                qso.potaReferences,
                qso.sotaReferences,
                qso.uploadStatus,
                qso.syncStatus,
                qso.notes
            ].map(HamRadioUtilities.csvEscaped).joined(separator: ",")
        }

        return ([header] + rows).joined(separator: "\n")
    }

    static func writeTemporaryExportFile(name: String, contents: String) throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(name)
        guard let data = contents.data(using: .utf8) else {
            throw CocoaError(.fileWriteUnknown, userInfo: [
                NSLocalizedDescriptionKey: "Unable to encode export contents as UTF-8."
            ])
        }
        try data.write(to: url, options: [.atomic])
        return url
    }

    private static func adifField(_ name: String, _ rawValue: String) -> String {
        let value = HamRadioUtilities.adifEscaped(rawValue)
        guard !value.isEmpty else { return "" }
        return "<\(name):\(value.count)>\(value)"
    }

    private static func portableSignalName(for qso: QSO) -> String {
        if !qso.potaReferences.isEmpty { return "POTA" }
        if !qso.sotaReferences.isEmpty { return "SOTA" }
        return ""
    }

    private static func portableSignalReference(for qso: QSO) -> String {
        if !qso.potaReferences.isEmpty { return qso.potaReferences }
        if !qso.sotaReferences.isEmpty { return qso.sotaReferences }
        return ""
    }
}
