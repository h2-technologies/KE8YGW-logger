import Foundation

enum LogExportService {
    static func adif(for qsos: [QSO]) -> String {
        var output = """
        KE8YGW Logger ADIF Export
        <ADIF_VER:5>3.1.4
        <PROGRAMID:12>KE8YGWLogger
        <PROGRAMVERSION:5>0.1.0
        <EOH>

        """

        for qso in qsos.sorted(by: { $0.contactDate < $1.contactDate }) {
            output += adifField("CALL", qso.callsign)
            output += adifField("QSO_DATE", HamDateFormatters.adifDate.string(from: qso.contactDate))
            output += adifField("TIME_ON", HamDateFormatters.adifTime.string(from: qso.contactDate))
            output += adifField("BAND", qso.band)
            output += adifField("MODE", qso.mode)
            if qso.frequencyMHz > 0 {
                output += adifField("FREQ", String(format: "%.6f", qso.frequencyMHz))
            }
            output += adifField("RST_SENT", qso.rstSent)
            output += adifField("RST_RCVD", qso.rstReceived)
            output += adifField("STATION_CALLSIGN", qso.stationCallsign)
            output += adifField("OPERATOR", qso.operatorCallsign)
            output += adifField("GRIDSQUARE", qso.gridSquare)
            output += adifField("NAME", qso.name)
            output += adifField("QTH", qso.qth)
            output += adifField("STATE", qso.state)
            output += adifField("COUNTRY", qso.country)
            output += adifField("COMMENT", qso.notes)
            output += "<EOR>\n"
        }

        return output
    }

    static func csv(for qsos: [QSO]) -> String {
        let header = [
            "Callsign", "DateTimeUTC", "Band", "Mode", "FrequencyMHz", "RSTSent",
            "RSTReceived", "Operator", "Station", "Grid", "Name", "QTH", "State",
            "Country", "Notes"
        ].joined(separator: ",")

        let rows = qsos.sorted(by: { $0.contactDate < $1.contactDate }).map { qso in
            [
                qso.callsign,
                HamDateFormatters.csvDateTime.string(from: qso.contactDate),
                qso.band,
                qso.mode,
                String(format: "%.6f", qso.frequencyMHz),
                qso.rstSent,
                qso.rstReceived,
                qso.operatorCallsign,
                qso.stationCallsign,
                qso.gridSquare,
                qso.name,
                qso.qth,
                qso.state,
                qso.country,
                qso.notes
            ].map(HamRadioUtilities.csvEscaped).joined(separator: ",")
        }

        return ([header] + rows).joined(separator: "\n")
    }

    static func writeTemporaryExportFile(name: String, contents: String) throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(name)
        try contents.data(using: .utf8)?.write(to: url, options: [.atomic])
        return url
    }

    private static func adifField(_ name: String, _ rawValue: String) -> String {
        let value = HamRadioUtilities.adifEscaped(rawValue)
        guard !value.isEmpty else { return "" }
        return "<\(name):\(value.count)>\(value)"
    }
}
