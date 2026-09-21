// Prefer guides. Keep specs and generated API references searchable at lower weight.
export function isSearchReference(id) {
  return /^(specs(?:\/|$)|reference\/(node|browser|agent)-sdk\/)/.test(id);
}

export const searchRanking = {
  pageLength: 0.1,
  termFrequency: 0.1,
  termSaturation: 2,
  termSimilarity: 9,
  metaWeights: { title: 0, guideTitle: 20 },
};
