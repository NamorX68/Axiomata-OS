<!--
  Settings → Appearance → "Language" (`docs/plans/i18n.md`, Q2, Q21): the system's language, German or English. The
  choice takes effect at once (Q14); the language's own name is written in that language.
-->
<script lang="ts">
  import { tSettings } from "../i18n/settings";
  import { isLanguageSetting, languageSetting, systemLang, type LanguageSetting } from "../i18n/lang.svelte";

  const name = (setting: LanguageSetting): string => {
    if (setting === "de") return tSettings("langDe");
    if (setting === "en") return tSettings("langEn");
    return tSettings("languageSystem", { lang: systemLang() === "de" ? tSettings("langDe") : tSettings("langEn") });
  };
</script>

<section>
  <h3>{tSettings("language")}</h3>
  <label class="row">
    <span class="label">{tSettings("language")}</span>
    <select
      value={$languageSetting}
      onchange={(e) => {
        const v = e.currentTarget.value;
        if (isLanguageSetting(v)) languageSetting.set(v);
      }}
    >
      {#each ["system", "de", "en"] as const as setting (setting)}
        <option value={setting}>{name(setting)}</option>
      {/each}
    </select>
  </label>
  <p class="hint">{tSettings("languageHint")}</p>
</section>

<style>
  /* As Settings' own sections draw them (`Settings.svelte`), so the row sits among Theme, Typeface and Motion. */
  h3 {
    font: var(--ax-type-label);
    color: var(--ax-text-muted);
    margin-bottom: var(--ax-space-2);
  }

  .row {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
    margin-bottom: var(--ax-space-1);
  }

  .label {
    flex: 0 0 auto;
  }

  .hint {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }
</style>
