/**
 * Preferenze da ripulire quando un brano lascia il disco: se restasse una
 * esclusione o un mood, tornerebbero in vita sul brano che un giorno riusera'
 * quel percorso.
 *
 * Si lancia con `pnpm test` (node --experimental-strip-types).
 */
import assert from "node:assert/strict";
import { test } from "node:test";

const store = new Map();
globalThis.localStorage = {
  getItem: (k) => (store.has(k) ? store.get(k) : null),
  setItem: (k, v) => store.set(k, String(v)),
  removeItem: (k) => store.delete(k),
  clear: () => store.clear(),
  key: (i) => [...store.keys()][i] ?? null,
  get length() {
    return store.size;
  },
};

const {
  prefsWithoutTracks,
  normalizeVisualizerMode,
  loadUserPrefs,
  patchUserPrefs,
  subscribeUserPrefsPatch,
  ALL_VISUALIZER_MODES,
} = await import("./userPrefs.ts");

/** Solo i campi che la funzione guarda: il resto delle preferenze non c'entra. */
function prefs(over = {}) {
  return {
    excludedRelPaths: [],
    excludedTrackIds: [],
    recentRelPaths: [],
    recentTrackIds: [],
    playCounts: {},
    trackMoods: {},
    ...over,
  };
}

const GONE = "Artist/Album/01.mp3";
const KEPT = "Artist/Album/02.mp3";

test("senza brani da dimenticare non tocca niente", () => {
  const patch = prefsWithoutTracks(prefs({ recentRelPaths: [GONE] }), { relPaths: [] });
  assert.deepEqual(patch, {});
});

test("il percorso sparisce da recenti, esclusioni, ascolti e mood", () => {
  const patch = prefsWithoutTracks(
    prefs({
      excludedRelPaths: [GONE, KEPT],
      recentRelPaths: [KEPT, GONE],
      playCounts: { [GONE]: 12, [KEPT]: 3 },
      trackMoods: { [GONE]: ["notte"], [KEPT]: ["festa"] },
    }),
    { relPaths: [GONE] },
  );

  assert.deepEqual(patch.excludedRelPaths, [KEPT]);
  assert.deepEqual(patch.recentRelPaths, [KEPT]);
  assert.deepEqual(patch.playCounts, { [KEPT]: 3 });
  assert.deepEqual(patch.trackMoods, { [KEPT]: ["festa"] });
});

test("anche le chiavi numeriche rimaste dall'import legacy se ne vanno", () => {
  const patch = prefsWithoutTracks(
    prefs({
      excludedTrackIds: [7, 9],
      recentTrackIds: [7, 9],
      playCounts: { 7: 4, [KEPT]: 1 },
      trackMoods: { 7: ["calmo"] },
    }),
    { relPaths: [GONE], trackIds: [7] },
  );

  assert.deepEqual(patch.excludedTrackIds, [9]);
  assert.deepEqual(patch.recentTrackIds, [9]);
  assert.deepEqual(patch.playCounts, { [KEPT]: 1 });
  assert.deepEqual(patch.trackMoods, {});
});

test("l'ordine dei recenti non cambia per chi resta", () => {
  const patch = prefsWithoutTracks(
    prefs({ recentRelPaths: ["a.mp3", GONE, "b.mp3", "c.mp3"] }),
    { relPaths: [GONE] },
  );
  assert.deepEqual(patch.recentRelPaths, ["a.mp3", "b.mp3", "c.mp3"]);
});

test("un album intero: tutti i suoi brani in un colpo", () => {
  const album = ["Artist/Album/01.mp3", "Artist/Album/02.mp3", "Artist/Album/CD2/01.mp3"];
  const patch = prefsWithoutTracks(
    prefs({
      recentRelPaths: [...album, "Other/Album/01.mp3"],
      playCounts: Object.fromEntries(album.map((p, i) => [p, i + 1])),
      trackMoods: Object.fromEntries(album.map((p) => [p, ["notte"]])),
    }),
    { relPaths: album },
  );

  assert.deepEqual(patch.recentRelPaths, ["Other/Album/01.mp3"]);
  assert.deepEqual(patch.playCounts, {});
  assert.deepEqual(patch.trackMoods, {});
});

test("DiscoWall salvato resta DiscoWall (non torna a barre)", () => {
  assert.equal(normalizeVisualizerMode("discowall"), "discowall");
  assert.ok(ALL_VISUALIZER_MODES.includes("discowall"));
  assert.equal(normalizeVisualizerMode("wave"), "osc");
  assert.equal(normalizeVisualizerMode("smooth"), "oscSoft");
  assert.equal(normalizeVisualizerMode("nebula"), "bars");
  assert.equal(normalizeVisualizerMode("plectr"), "bars");
  assert.equal(normalizeVisualizerMode("boh"), "bars");
  patchUserPrefs({ visualizerMode: "discowall" });
  assert.equal(loadUserPrefs().visualizerMode, "discowall");
});

test("chi ascolta le patch riceve solo i campi cambiati", () => {
  const seen = [];
  const off = subscribeUserPrefsPatch((patch) => seen.push(Object.keys(patch)));
  patchUserPrefs({ playCounts: { [KEPT]: 2 } });
  off();
  patchUserPrefs({ playCounts: { [KEPT]: 3 } });
  assert.deepEqual(seen, [["playCounts"]]);
});

test("cache: leggere due volte non riparsa, e le raccolte condivise sono di sola lettura", async () => {
  const { getPlayCountsMap } = await import("./userPrefs.ts");
  patchUserPrefs({ playCounts: { [KEPT]: 4 } });
  const a = getPlayCountsMap();
  const b = getPlayCountsMap();
  assert.equal(a, b);
  assert.throws(() => {
    "use strict";
    a[KEPT] = 99;
  });
  assert.equal(loadUserPrefs().playCounts[KEPT], 4);
});

test("un account nuovo parte vuoto: niente copiato da default", async () => {
  const { adoptPrefsForAccount } = await import("./userPrefs.ts");
  patchUserPrefs({ playCounts: { [KEPT]: 7 } }, "default");
  adoptPrefsForAccount("fresh-account");
  assert.deepEqual(loadUserPrefs("fresh-account").playCounts, {});
  assert.deepEqual(loadUserPrefs("fresh-account").recentRelPaths, []);
});

test("un'altra scheda riscrive le preferenze: la cache si svuota", async () => {
  const { invalidateUserPrefsCache } = await import("./userPrefs.ts");
  patchUserPrefs({ playCounts: { [KEPT]: 1 } }, "tab-acc");
  store.set("rekord.next.userPrefs.tab-acc", JSON.stringify({ playCounts: { [KEPT]: 9 } }));
  assert.equal(loadUserPrefs("tab-acc").playCounts[KEPT], 1);
  invalidateUserPrefsCache("tab-acc");
  assert.equal(loadUserPrefs("tab-acc").playCounts[KEPT], 9);
});

test("locale: it / en / de are kept, anything else falls back to Italian", async () => {
  const { normalizeLocale, localeFromTag, browserLocale } = await import("./userPrefs.ts");
  assert.equal(normalizeLocale("de"), "de");
  assert.equal(normalizeLocale("en"), "en");
  assert.equal(normalizeLocale("fr"), "it");
  assert.equal(normalizeLocale(undefined), "it");
  assert.equal(localeFromTag("de-AT"), "de");
  assert.equal(localeFromTag("EN_us"), "en");
  assert.equal(localeFromTag("fr-FR"), null);
  // First supported browser language wins; Italian when none matches.
  assert.equal(browserLocale(["fr-FR", "de-CH", "en"]), "de");
  assert.equal(browserLocale(["en-GB", "de"]), "en");
  assert.equal(browserLocale(["fr", "es"]), "it");
  assert.equal(browserLocale([]), "it");
});

test("saving live $state collections (edit-track moods) never freezes the proxy", async () => {
  // Regression: the track edit dialog saved `{ [relPath]: draftMoods }` with
  // `draftMoods` a `$state` array; freezing the cached copy froze the proxy
  // itself and Svelte threw `state_descriptors_fixed`, so nothing was saved.
  const { proxy } = await import("svelte/internal/client");
  const draftMoods = proxy(["chill_relax", "focus_study"]);
  const excluded = proxy([KEPT]);
  assert.throws(() => Object.freeze(proxy(["x"])), /state_descriptors_fixed/);
  assert.doesNotThrow(() =>
    patchUserPrefs({
      trackMoods: { ...loadUserPrefs().trackMoods, [KEPT]: draftMoods },
      excludedRelPaths: excluded,
    }),
  );
  const saved = loadUserPrefs();
  assert.deepEqual([...saved.trackMoods[KEPT]], ["chill_relax", "focus_study"]);
  assert.deepEqual([...saved.excludedRelPaths], [KEPT]);
  assert.ok(Object.isFrozen(saved.trackMoods[KEPT]));
  // The caller's live state stays editable.
  draftMoods.push("party_dance");
  assert.equal(draftMoods.length, 3);
  assert.deepEqual([...loadUserPrefs().trackMoods[KEPT]], ["chill_relax", "focus_study"]);
});
