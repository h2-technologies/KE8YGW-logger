import CoreLocation
import MapKit
import SwiftUI

struct MapWorkspaceView: View {
    @EnvironmentObject private var bridge: RustBridgeStore
    @State private var position: MapCameraPosition = .automatic

    private var coordinate: CLLocationCoordinate2D {
        CLLocationCoordinate2D(
            latitude: bridge.map.status.coordinates.latitude,
            longitude: bridge.map.status.coordinates.longitude
        )
    }

    var body: some View {
        VStack(spacing: 0) {
            Map(position: $position) {
                Marker("Station", systemImage: "antenna.radiowaves.left.and.right", coordinate: coordinate)
            }
            .mapControls {
                MapCompass()
                MapScaleView()
                MapUserLocationButton()
            }
            .frame(minHeight: 320)

            List {
                Section("Status") {
                    DetailRow(title: "Grid", value: bridge.map.status.grid)
                    DetailRow(title: "Coordinates", value: "\(bridge.map.status.coordinates.latitude), \(bridge.map.status.coordinates.longitude)")
                    DetailRow(title: "Distance", value: bridge.map.status.distance)
                    DetailRow(title: "Bearing", value: bridge.map.status.bearing)
                    DetailRow(title: "Layer", value: bridge.map.status.selectedLayer)
                }

                Section("Layers") {
                    Label("Current location", systemImage: "location")
                    Label("QSO pins", systemImage: "mappin.and.ellipse")
                    Label("Maidenhead grids", systemImage: "grid")
                    Label("POTA parks", systemImage: "tree")
                    Label("SOTA summits", systemImage: "mountain.2")
                    Label("Cluster spots", systemImage: "scope")
                }

                Section("Navigation") {
                    Button("Center Station") {
                        position = .region(MKCoordinateRegion(
                            center: coordinate,
                            span: MKCoordinateSpan(latitudeDelta: 2, longitudeDelta: 2)
                        ))
                    }
                    Button("Refresh Rust Map Snapshot") {
                        Task { await bridge.refreshMap() }
                    }
                }
            }
        }
        .navigationTitle("Maps")
        .task {
            await bridge.refreshMap()
            position = .region(MKCoordinateRegion(
                center: coordinate,
                span: MKCoordinateSpan(latitudeDelta: 2, longitudeDelta: 2)
            ))
        }
    }
}
