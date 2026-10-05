/* RE-KORD service worker — generato da vite.config.ts (plugin rekordServiceWorker)
 * a partire da src/lib/platform/serviceWorker.template.js. Non modificare dist/sw.js.
 *
 * Solo il guscio dell'app: index.html e i file statici della build. API, audio,
 * copertine e pannello admin vanno sempre in rete (mai dalla cache): sono dati
 * dell'hub, e una copia vecchia sarebbe peggio di un errore.
 */
const VERSION = "__REKORD_SW_VERSION__";
const PRECACHE = __REKORD_SW_PRECACHE__;
const CACHE = `rekord-shell-${VERSION}`;
const NETWORK_ONLY = [/^\/api(\/|$)/, /^\/media(\/|$)/, /^\/admin(\/|$)/];

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches
      .open(CACHE)
      .then((cache) => cache.addAll(PRECACHE))
      .then(() => self.skipWaiting()),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) =>
        Promise.all(
          keys
            .filter((k) => k.startsWith("rekord-shell-") && k !== CACHE)
            .map((k) => caches.delete(k)),
        ),
      )
      .then(() => self.clients.claim()),
  );
});

self.addEventListener("fetch", (event) => {
  const req = event.request;
  if (req.method !== "GET") return;
  const url = new URL(req.url);
  if (url.origin !== self.location.origin) return;
  if (NETWORK_ONLY.some((re) => re.test(url.pathname))) return;
  // Richieste a pezzi (Range) non passano dalla cache.
  if (req.headers.has("range")) return;

  if (req.mode === "navigate") {
    // Rete prima: un hub aggiornato serve subito la nuova interfaccia. Offline,
    // l'ultima index.html in cache (l'indicatore di connessione fa il resto).
    event.respondWith(
      fetch(req)
        .then((res) => {
          if (res.ok) {
            const copy = res.clone();
            caches.open(CACHE).then((c) => c.put("/index.html", copy));
          }
          return res;
        })
        .catch(() =>
          caches.match("/index.html").then((hit) => hit || Response.error()),
        ),
    );
    return;
  }

  // File della build: hanno l'hash nel nome, quindi la cache non invecchia mai.
  event.respondWith(
    caches.match(req).then(
      (hit) =>
        hit ||
        fetch(req).then((res) => {
          if (res.ok && url.pathname.startsWith("/assets/")) {
            const copy = res.clone();
            caches.open(CACHE).then((c) => c.put(req, copy));
          }
          return res;
        }),
    ),
  );
});
