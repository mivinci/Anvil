<script lang="ts">
	import { ArrowRight, Check, Monitor, Moon, Sun } from '@lucide/svelte';
	import { localeState } from '#lib/locale.svelte.ts';
	import { getAppearance, themePresets, themePresetName } from '../appearance';
	import LanguageSelect from './language-select.svelte';
	import Button from './ui/button.svelte';

	let { oncomplete, storageFailed = false }: { oncomplete: () => void; storageFailed?: boolean } = $props();
	const appearance = getAppearance();
	const t = $derived(localeState.messages);
	const modes = $derived([
		{ value: 'system', label: t.settings.flow.system, icon: Monitor },
		{ value: 'light', label: t.settings.flow.light, icon: Sun },
		{ value: 'dark', label: t.settings.flow.dark, icon: Moon },
	] as const);
</script>

<svelte:head><title>{t.common.pageTitle(t.welcome.title)}</title></svelte:head>

<main aria-labelledby="welcome-title" class="flex min-h-0 flex-1 flex-col overflow-y-auto bg-main-pane text-foreground">
	<div class="mx-auto flex w-full max-w-md flex-1 flex-col justify-center gap-6 px-8 py-8">
		<header class="text-center">
			<h1 id="welcome-title" class="text-xl font-semibold tracking-tight">{t.welcome.title}</h1>
			<p class="mt-2 text-xs leading-relaxed text-muted-foreground">{t.welcome.description}</p>
		</header>

		<div class="space-y-5">
				<div class="flex flex-wrap items-center justify-between gap-3">
					<label id="welcome-language-title" for="welcome-language" class="text-sm font-medium">{t.settings.language}</label>
					<LanguageSelect id="welcome-language" labelledby="welcome-language-title" />
				</div>
				<fieldset>
					<legend class="mb-2 text-sm font-medium">{t.settings.flow.displayMode}</legend>
					<div class="grid grid-cols-3 gap-2">
						{#each modes as mode}
							{@const Icon = mode.icon}
							<button type="button" aria-pressed={appearance.mode === mode.value} onclick={() => appearance.setMode(mode.value)} class={`flex min-h-16 flex-col items-center justify-center gap-2 rounded-xl px-2 py-3 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring ${appearance.mode === mode.value ? 'bg-primary/10 text-primary ring-1 ring-primary' : 'bg-card hover:bg-card-hover'}`}><Icon size={17} aria-hidden="true" />{mode.label}</button>
						{/each}
					</div>
				</fieldset>
				<fieldset>
					<legend class="mb-2 text-sm font-medium">{t.welcome.theme}</legend>
					<div class="grid grid-cols-2 gap-2">
						{#each themePresets as preset}
							<button type="button" aria-pressed={appearance.colors.theme === preset.id} onclick={() => appearance.setTheme(preset.id)} class={`flex min-w-0 items-center gap-2 rounded-xl bg-card px-3 py-2.5 text-left text-xs outline-none hover:bg-card-hover focus-visible:ring-2 focus-visible:ring-ring ${appearance.colors.theme === preset.id ? 'ring-1 ring-primary' : ''}`}>
								<span class="size-4 shrink-0 rounded-full" style:background={preset[appearance.resolvedMode].accent} aria-hidden="true"></span>
								<span class="min-w-0 flex-1">{themePresetName(preset.id, localeState.current)}</span>
								{#if appearance.colors.theme === preset.id}<Check size={12} class="shrink-0 text-primary" aria-hidden="true" />{/if}
							</button>
						{/each}
					</div>
				</fieldset>
		</div>

		<footer class="flex flex-col items-center gap-3">
			{#if localeState.storageMessage}<p role="status" class="text-center text-xs text-muted-foreground">{localeState.storageMessage}</p>{/if}
			{#if appearance.saveError}<p role="status" class="text-center text-xs text-destructive">{appearance.saveError}</p>{/if}
			{#if storageFailed}<p role="status" class="text-center text-xs text-muted-foreground">{t.welcome.storageFailed}</p>{/if}
			<Button onclick={oncomplete} class="min-w-40 gap-2">{t.welcome.start}<ArrowRight size={15} aria-hidden="true" /></Button>
			<p class="text-center text-xs text-muted-foreground">{t.welcome.hint}</p>
		</footer>
	</div>
</main>
