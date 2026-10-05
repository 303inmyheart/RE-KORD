/**
 * Default visible / accessible labels for the shared components. The package
 * has no i18n of its own: the app pushes its translations here (and again on a
 * locale change); a prop on a single component (`closeLabel`, `message`,
 * `placeholder`, `buttonLabel`, `backLabel`) still wins.
 */
export const uiLabels = $state({
  close: "Chiudi",
  /** EmptyState default message. */
  empty: "Nessun elemento",
  /** SearchBar default placeholder. */
  searchPlaceholder: "Cerca…",
  /** SearchBar default button label. */
  search: "Cerca",
  /** SectionHeader default back button label. */
  back: "← Indietro",
});

export function setUiLabels(next: Partial<typeof uiLabels>) {
  Object.assign(uiLabels, next);
}
