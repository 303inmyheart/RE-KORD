<script lang="ts">
  import { onMount } from "svelte";
  import { Button, Modal } from "@rekord/ui";
  import { confirmStore } from "../lib/confirm.svelte";
  import { t } from "../lib/i18n.svelte";

  const req = $derived(confirmStore.current);

  onMount(() => {
    confirmStore.hosts += 1;
    return () => {
      confirmStore.hosts -= 1;
      if (confirmStore.hosts <= 0) confirmStore.cancelAll();
    };
  });
</script>

{#if req}
  {#key req.id}
    <Modal
      open
      title={req.title}
      panelClass="rk-confirm"
      onclose={() => confirmStore.settle(req.id, false)}
    >
      {#if req.message}
        <p class="msg">{req.message}</p>
      {/if}
      {#snippet footer()}
        <!-- Destructive: focus starts on Cancel, so Enter never deletes by accident. -->
        <Button
          variant="ghost"
          data-autofocus={req.danger ? "" : undefined}
          onclick={() => confirmStore.settle(req.id, false)}
        >
          {req.cancelLabel ?? t("ui.confirm.cancel")}
        </Button>
        <Button
          variant="primary"
          tone={req.danger ? "danger" : "default"}
          data-autofocus={req.danger ? undefined : ""}
          onclick={() => confirmStore.settle(req.id, true)}
        >
          {req.confirmLabel ?? t("ui.confirm.ok")}
        </Button>
      {/snippet}
    </Modal>
  {/key}
{/if}

<style>
  .msg {
    margin: 0;
    color: var(--rk-muted);
    font-size: var(--rk-fs-sm);
    line-height: var(--rk-lh-snug, 1.4);
    white-space: pre-line;
  }
</style>
