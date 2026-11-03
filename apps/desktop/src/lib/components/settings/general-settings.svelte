<script lang="ts">
	import { localeNames, locales } from '#lib/i18n/index.ts';
	import { localeState, type LocalePreference } from '#lib/locale.svelte.ts';
</script>

<section class="space-y-2" aria-labelledby="language-title">
	<div class="flex flex-wrap items-center justify-between gap-3 rounded-lg bg-card px-3.5 py-3">
		<div class="min-w-0 flex-1">
			<label id="language-title" for="app-language" class="text-[13px] font-medium">{localeState.messages.settings.language}</label>
			<p id="language-description" class="mt-1 text-[11px] leading-5 text-muted-foreground">{localeState.messages.settings.languageDescription}</p>
		</div>
		<select id="app-language" value={localeState.preference} aria-describedby="language-description" onchange={(event) => localeState.setLocale(event.currentTarget.value as LocalePreference)} class="h-8 max-w-full shrink-0 rounded-md border border-border bg-main-pane px-2 text-xs text-foreground focus-visible:outline-2 focus-visible:outline-ring">
			<option value="system">{localeState.messages.settings.systemLanguage}</option>
			{#each locales as locale}<option value={locale} lang={locale}>{localeNames[locale]}</option>{/each}
		</select>
	</div>
	{#if localeState.storageMessage}<p role="status" class="px-1 text-[11px] leading-5 text-muted-foreground">{localeState.storageMessage}</p>{/if}
</section>
