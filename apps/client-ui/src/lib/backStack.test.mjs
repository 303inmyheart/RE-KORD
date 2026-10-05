/**
 * Back stack: dialoghi chiusi da codice non devono lasciare voci morte nella
 * cronologia (il Back successivo sembrerebbe non fare niente).
 * Il modulo vive in @rekord/ui; history e popstate sono simulati qui, con la
 * navigazione asincrona come nei browser.
 */
import assert from "node:assert/strict";
import { afterEach, beforeEach, test } from "node:test";

function installFakeHistory() {
  const bus = new EventTarget();
  const entries = [{ state: null }];
  let index = 0;
  globalThis.window = {
    addEventListener: (type, fn) => bus.addEventListener(type, fn),
    removeEventListener: (type, fn) => bus.removeEventListener(type, fn),
  };
  globalThis.history = {
    get state() {
      return entries[index].state;
    },
    pushState(state) {
      entries.length = index + 1;
      entries.push({ state: structuredClone(state) });
      index += 1;
    },
    replaceState(state) {
      entries[index] = { state: structuredClone(state) };
    },
    back() {
      // Like browsers: the traversal (and `history.state`) changes later.
      setTimeout(() => {
        if (index === 0) return;
        index -= 1;
        const ev = new Event("popstate");
        ev.state = entries[index].state;
        bus.dispatchEvent(ev);
      }, 0);
    },
  };
  return {
    get index() {
      return index;
    },
    get length() {
      return entries.length;
    },
  };
}

const settle = () => new Promise((r) => setTimeout(r, 20));

const { enableBackStack, onHistoryPop, pushBackLayer, backLayerCount } = await import(
  "../../../../packages/ui/src/lib/backStack.ts"
);

let fake;
let teardown;

beforeEach(() => {
  fake = installFakeHistory();
  teardown = enableBackStack();
});

afterEach(() => {
  teardown();
});

test("due dialoghi chiusi insieme (prima quello sopra): si torna alla voce di partenza", async () => {
  const releaseBottom = pushBackLayer(() => {});
  const releaseTop = pushBackLayer(() => {});
  assert.equal(fake.index, 2);
  releaseTop();
  releaseBottom();
  await settle();
  assert.equal(backLayerCount(), 0);
  assert.equal(fake.index, 0);
});

test("dialogo sotto chiuso da codice, poi quello sopra: nessuna voce orfana", async () => {
  const releaseBottom = pushBackLayer(() => {});
  const releaseTop = pushBackLayer(() => {});
  releaseBottom();
  releaseTop();
  await settle();
  assert.equal(fake.index, 0);
});

test("Back chiude il dialogo sopra e salta la voce orfana senza navigare", async () => {
  const pops = [];
  const off = onHistoryPop((s) => pops.push(s));
  let topClosed = 0;
  const releaseBottom = pushBackLayer(() => {});
  pushBackLayer(() => {
    topClosed += 1;
  });
  releaseBottom();
  history.back(); // tasto Back dell'utente
  await settle();
  off();
  assert.equal(topClosed, 1);
  assert.equal(fake.index, 0);
  assert.deepEqual(pops, []);
});

test("un solo dialogo chiuso da codice: una sola voce tolta", async () => {
  const release = pushBackLayer(() => {});
  assert.equal(fake.index, 1);
  release();
  await settle();
  assert.equal(fake.index, 0);
});
