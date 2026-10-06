<script lang="ts">
  /**
   * Section tabs of a page (Library: Artisti / Generi / Mood / Nebula, Settings,
   * Studio). Thin wrapper over `@rekord/ui` `Tabs`: one line, scrolls sideways
   * with an edge fade on narrow screens instead of wrapping.
   */
  import { Tabs } from "@rekord/ui";
  import { t } from "../lib/i18n.svelte";

  type Tab = { id: string; label: string; count?: number | string | null };

  let {
    tabs,
    active,
    ariaLabel,
    size = "md" as "md" | "nav" | "sm",
    even = false,
    onselect,
  }: {
    tabs: Tab[];
    active: string;
    ariaLabel?: string;
    /** `md` page-level (title size), `nav` under a page title, `sm` inside a panel. */
    size?: "md" | "nav" | "sm";
    /** Spreads the tabs across the full width (e.g. Studio); Library stays flex-start */
    even?: boolean;
    onselect: (id: string) => void;
  } = $props();

  const tabsSize = $derived(size === "md" ? "lg" : size === "nav" ? "md" : "sm");
</script>

<Tabs
  class="section-nav-tabs"
  tabClass="section-nav-tab"
  items={tabs}
  {active}
  size={tabsSize}
  {even}
  ariaLabel={ariaLabel ?? t("sectionTabs.aria")}
  {onselect}
/>
