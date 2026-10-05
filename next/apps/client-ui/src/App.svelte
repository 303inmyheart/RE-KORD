<script lang="ts" module>
  import { applyPlatformCapsToDom } from "./lib/platformCaps";

  // Before the first paint: CSS needs `data-rk-lowfx` on WebKitGTK from frame one.
  applyPlatformCapsToDom();
</script>

<script lang="ts">
  import { onMount } from "svelte";
  import AppShell from "./components/AppShell.svelte";
  import BootSplash from "./components/BootSplash.svelte";
  import ConnectScreen from "./components/ConnectScreen.svelte";
  import { getSelectedAccountId, setSelectedAccountId } from "./lib/account";
  import { connectGate } from "./lib/connect.svelte";
  import { getServerBaseUrl, setServerBaseUrl } from "./lib/config";
  import { session } from "./lib/session.svelte";

  /** L'app parte una volta sola, anche se si passa dalla procedura di connessione. */
  let started = $state(false);

  function start() {
    if (started) return;
    started = true;
    // Il gate puo' aver appena scelto l'hub locale: il campo in Impostazioni deve
    // mostrare l'indirizzo su cui stiamo davvero parlando.
    session.serverUrl = getServerBaseUrl();
    void session.bootstrap();
  }

  onMount(() => {
    const unbindPlayer = session.bindPlayer();
    const unbindWindow = session.bindWindow();
    void connectGate.decideOnStart().then((ready) => {
      if (ready) start();
    });
    return () => {
      unbindPlayer();
      unbindWindow();
    };
  });

  function onConnected(base: string, accountId: string) {
    setServerBaseUrl(base);
    session.serverUrl = base;
    connectGate.close();
    if (!started) {
      setSelectedAccountId(accountId);
      start();
      return;
    }
    // Procedura riaperta con l'app in piedi: cambiare account non e' solo scrivere
    // un id — vanno salvate le preferenze di quello che lascia e ricaricati tema,
    // esclusioni e selezione della libreria. Se e' lo stesso account basta rileggere.
    if (accountId !== getSelectedAccountId()) void session.switchAccount(accountId);
    else void session.refreshAll();
  }
</script>

{#if connectGate.phase === "connect"}
  <ConnectScreen
    savedBase={connectGate.savedBase}
    dismissible={started}
    onconnected={onConnected}
    ondismiss={() => connectGate.close()}
  />
{:else if connectGate.phase === "probing"}
  <!-- Sonda all'avvio, di solito qualche millisecondo: prima solo il marchio;
       barra e suggerimenti compaiono se l'attesa si allunga. -->
  <BootSplash />
{:else}
  <AppShell />
{/if}
