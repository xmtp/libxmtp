import SwiftUI

struct GroupFieldsFeatureView: View {
	let graph: AppGraph
	let id: String
	var body: some View {
		MetadataView(graph: graph, id: id, own: false)
	}
}

#Preview("Light") { DetailsPreviewHost { graph in MetadataView(
	preview: true,
	graph: graph,
	id: "preview-group",
	own: false,
) } }

#Preview("Dark") { DetailsPreviewHost { graph in MetadataView(
	preview: true,
	graph: graph,
	id: "preview-group",
	own: false,
) }.preferredColorScheme(.dark) }

#Preview("Large text") { DetailsPreviewHost { graph in MetadataView(
	preview: true,
	graph: graph,
	id: "preview-group",
	own: false,
) }.dynamicTypeSize(.accessibility3) }
