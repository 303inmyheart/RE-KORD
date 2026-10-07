/**
 * Hub panel › Podcast e notizie: the module switch, the cache TTL and the
 * sources (add with a preview, rename, episodes per source, order, remove).
 * Messages go to the panel's shared banners.
 */
import { api, errorText, type PodcastPreview, type PodcastsAdmin } from "../api";
import { admin } from "./admin.svelte";
import { t } from "./i18n.svelte";

class PodcastsAdminStore {
  data = $state<PodcastsAdmin | null>(null);
  busy = $state(false);
  /** New source form. */
  url = $state("");
  name = $state("");
  count = $state(3);
  testing = $state(false);
  preview = $state<PodcastPreview | null>(null);
  /** Translated error of the last test / add (shown next to the form). */
  formError = $state("");
  ttl = $state(30);

  async load() {
    try {
      this.apply(await api.podcasts());
    } catch (e) {
      admin.error = errorText(e);
    }
  }

  private apply(data: PodcastsAdmin) {
    this.data = data;
    this.ttl = data.cacheTtlMinutes;
    if (!this.url) this.count = data.limits.defaultEpisodes;
  }

  private async run(fn: () => Promise<string | void>) {
    this.busy = true;
    admin.error = "";
    admin.message = "";
    try {
      const msg = await fn();
      if (msg) admin.message = msg;
    } catch (e) {
      admin.error = errorText(e);
    } finally {
      this.busy = false;
    }
  }

  setEnabled(enabled: boolean) {
    return this.run(async () => {
      this.apply(await api.setPodcastSettings({ enabled }));
      return enabled ? t("podcasts.msg.enabled") : t("podcasts.msg.disabled");
    });
  }

  saveTtl() {
    return this.run(async () => {
      this.apply(await api.setPodcastSettings({ cacheTtlMinutes: Math.round(this.ttl) }));
      return t("podcasts.msg.ttlSaved", { minutes: this.data?.cacheTtlMinutes ?? this.ttl });
    });
  }

  /** Preview of the URL in the form: detected type and latest episodes. */
  async test() {
    const url = this.url.trim();
    if (!url) return;
    this.testing = true;
    this.formError = "";
    this.preview = null;
    try {
      this.preview = await api.testPodcastSource(url, this.count);
      if (!this.name.trim() && this.preview.title) this.name = this.preview.title;
    } catch (e) {
      this.formError = errorText(e);
    } finally {
      this.testing = false;
    }
  }

  add() {
    const url = this.url.trim();
    if (!url) return Promise.resolve();
    this.formError = "";
    return this.run(async () => {
      const preview = this.preview;
      const name = this.name.trim();
      try {
        const src = await api.addPodcastSource({
          url,
          // A name equal to the detected title stays automatic (follows the feed).
          name: name && name !== preview?.title ? name : undefined,
          episodeCount: this.count,
        });
        this.url = "";
        this.name = "";
        this.preview = null;
        await this.load();
        return t("podcasts.msg.added", { name: src.name });
      } catch (e) {
        this.formError = errorText(e);
        return;
      }
    });
  }

  rename(id: number, name: string) {
    return this.run(async () => {
      await api.updatePodcastSource(id, { name });
      await this.load();
      return t("podcasts.msg.saved");
    });
  }

  setCount(id: number, episodeCount: number) {
    return this.run(async () => {
      await api.updatePodcastSource(id, { episodeCount });
      await this.load();
      return t("podcasts.msg.saved");
    });
  }

  move(id: number, delta: -1 | 1) {
    const ids = (this.data?.sources ?? []).map((s) => s.id);
    const from = ids.indexOf(id);
    const to = from + delta;
    if (from < 0 || to < 0 || to >= ids.length) return Promise.resolve();
    [ids[from], ids[to]] = [ids[to]!, ids[from]!];
    return this.run(async () => {
      this.apply(await api.orderPodcastSources(ids));
    });
  }

  remove(id: number, name: string) {
    return this.run(async () => {
      await api.deletePodcastSource(id);
      await this.load();
      return t("podcasts.msg.removed", { name });
    });
  }
}

export const podcastsAdmin = new PodcastsAdminStore();
