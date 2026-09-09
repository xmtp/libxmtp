// Prefer guide titles. Keep generated API names searchable in their page bodies.
export const searchRanking = {
  pageLength: 0.1,
  termFrequency: 0.1,
  termSaturation: 2,
  termSimilarity: 9,
  metaWeights: { title: 0, guideTitle: 20 },
};
