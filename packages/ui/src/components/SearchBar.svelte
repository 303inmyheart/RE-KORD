<script lang="ts">
  import Button from "./Button.svelte";
  import TextInput from "./TextInput.svelte";
  import ActionRow from "./ActionRow.svelte";
  import { uiLabels } from "../lib/uiLabels.svelte";

  let {
    value = $bindable(""),
    placeholder,
    buttonLabel,
    onsearch,
    oninput,
  }: {
    value?: string;
    placeholder?: string;
    buttonLabel?: string;
    onsearch?: () => void;
    oninput?: () => void;
  } = $props();
</script>

<ActionRow>
  <TextInput
    type="search"
    bind:value
    placeholder={placeholder ?? uiLabels.searchPlaceholder}
    oninput={() => oninput?.()}
    onkeydown={(e) => e.key === "Enter" && onsearch?.()}
  />
  <Button variant="ghost" onclick={() => onsearch?.()}>{buttonLabel ?? uiLabels.search}</Button>
</ActionRow>
