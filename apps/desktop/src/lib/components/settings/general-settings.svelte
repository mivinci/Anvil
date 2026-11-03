<script lang="ts">
	import LanguageSelect from '../language-select.svelte';
	import { localeState } from '#lib/locale.svelte.ts';
</script>

<section class="space-y-2" aria-labelledby="language-title">
	<div class="flex flex-wrap items-center justify-between gap-3 rounded-lg bg-card px-3.5 py-3">
		<div class="min-w-0 flex-1">
			<label id="language-title" for="app-language" class="text-[13px] font-medium">{localeState.messages.settings.language}</label>
			<p id="language-description" class="mt-1 text-[11px] leading-5 text-muted-foreground">{localeState.messages.settings.languageDescription}</p>
		</div>
		<LanguageSelect id="app-language" labelledby="language-title" describedby="language-description" />
	</div>
	{#if localeState.storageMessage}<p role="status" class="px-1 text-[11px] leading-5 text-muted-foreground">{localeState.storageMessage}</p>{/if}
</section>
