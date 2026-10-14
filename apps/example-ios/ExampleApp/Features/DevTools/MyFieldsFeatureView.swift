import SwiftUI

struct MyFieldsFeatureView: View {
	let graph: AppGraph
	let id: String
	var body: some View {
		MetadataView(graph: graph, id: id, own: true)
	}
}

#Preview("Light") { DetailsPreviewHost { graph in MetadataView(
	preview: true,
	graph: graph,
	id: "preview-group",
	own: true,
) } }

#Preview("Dark") { DetailsPreviewHost { graph in MetadataView(
	preview: true,
	graph: graph,
	id: "preview-group",
	own: true,
) }.preferredColorScheme(.dark) }

#Preview("Large text") { DetailsPreviewHost { graph in MetadataView(
	preview: true,
	graph: graph,
	id: "preview-group",
	own: true,
) }.dynamicTypeSize(.accessibility3) }
