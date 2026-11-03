<script lang="ts">
	import { localeState } from '#lib/locale.svelte.js';
	import { Monitor, Moon, Sun } from '@lucide/svelte';
	import { getAppearance, getThemePreset, themePresets, themePresetName, type ColorMode, type Palette, type ThemePreferences } from '../../appearance';
	import ColorControl from './color-control.svelte';
	const appearance = getAppearance();
	let editingMode = $state<ColorMode>('light');
	let editing = $state(false);
	const colorMode = $derived(editing ? editingMode : appearance.resolvedMode);
	const presetPalette = $derived(getThemePreset(appearance.colors.theme)[appearance.resolvedMode]);
	const swatches = ['#827099', '#b5a0ce', '#688c83', '#7393b3', '#b88472'];
	const textScales = $derived([
		{ value: 'default', label: localeState.messages.settings.flow.textSizeDefault },
		{ value: 'large', label: localeState.messages.settings.flow.textSizeLarge },
		{ value: 'larger', label: localeState.messages.settings.flow.textSizeLarger },
	] as const);
	const modes = $derived([
		{ value: 'system', label: localeState.messages.settings.flow.system, icon: Monitor },
		{ value: 'light', label: localeState.messages.settings.flow.light, icon: Sun },
		{ value: 'dark', label: localeState.messages.settings.flow.dark, icon: Moon },
	] as const);
</script>

<div class="space-y-5">
	<section aria-labelledby="visual-style" class="space-y-2">
		<h2 id="visual-style" class="text-body font-medium">{localeState.messages.settings.flow.visualStyle}</h2>
		<div class="flex flex-wrap items-center justify-between gap-x-4 gap-y-3 rounded-lg bg-card px-3.5 py-3">
			<div class="flex items-center gap-3">
				<div><h3 class="text-body font-medium">{localeState.messages.settings.flow.displayMode}</h3></div>
			</div>
			<div class="grid shrink-0 grid-cols-3 gap-2" role="group" aria-label={localeState.messages.settings.flow.displayMode}>
				{#each modes as mode}
					{@const Icon = mode.icon}
					<button type="button" aria-pressed={appearance.mode === mode.value} onclick={() => appearance.setMode(mode.value)} class={`group w-21 rounded-md p-1 text-left transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring ${appearance.mode === mode.value ? 'bg-primary/8 ring-2 ring-primary' : 'hover:bg-chrome-hover'}`}>
						<div aria-hidden="true" class={`relative isolate flex h-12 overflow-hidden rounded-md ${mode.value === 'dark' ? 'bg-[#303133]' : mode.value === 'light' ? 'bg-[#e7e7e5]' : 'bg-[#e7e7e5]'}`}>
							{#if mode.value === 'system'}<span class="pointer-events-none absolute inset-y-0 right-0 -z-10 w-1/2 bg-[#303133]"></span>{/if}
							<div class="flex w-1/4 flex-col items-center gap-1.5 pt-2"><span class="size-1.5 rounded-sm bg-neutral-500/40"></span><span class="size-1.5 rounded-sm bg-primary/60"></span><span class="size-1.5 rounded-sm bg-neutral-500/40"></span></div>
							<!-- The sidebar occupies 1/4 of the outer width; include the right margin when mapping its midpoint into the content pane. -->
							<div class={`relative isolate my-1.5 mr-1.5 flex flex-1 flex-col gap-1.5 overflow-hidden rounded-sm p-1.5 ${mode.value === 'dark' ? 'bg-[#191a1b]' : mode.value === 'light' ? 'bg-white' : 'bg-white'}`}>{#if mode.value === 'system'}<span class="pointer-events-none absolute inset-y-0 right-0 left-[calc((100%+0.375rem)/3)] -z-10 bg-[#191a1b]"></span>{/if}<span class="h-0.5 w-2/3 rounded-full bg-neutral-400/40"></span><span class="h-0.5 w-1/2 rounded-full bg-neutral-400/25"></span><span class="mt-auto h-1.5 w-1/3 self-end rounded-sm bg-primary"></span></div>
						</div>
						<span class="mt-1.5 flex items-center justify-center gap-1 pb-0.5 text-xs"><Icon size={12} aria-hidden="true" />{mode.label}</span>
					</button>
				{/each}
			</div>
		</div>
	</section>
	<section aria-labelledby="text-size" class="space-y-2">
		<h2 id="text-size" class="text-body font-medium">{localeState.messages.settings.flow.textSize}</h2>
		<div class="flex flex-wrap items-center justify-between gap-3 rounded-lg bg-card px-3.5 py-3">
			<div class="min-w-0">
				<h3 class="text-body font-medium">{localeState.messages.settings.flow.textSize}</h3>
				<p class="mt-1 text-xs text-muted-foreground">{localeState.messages.settings.flow.textSizeHint}</p>
			</div>
			<div class="inline-flex rounded-[10px] bg-chrome-hover p-0.75" role="group" aria-label={localeState.messages.settings.flow.textSize}>
				{#each textScales as scale}
					<button type="button" aria-pressed={appearance.uiScale === scale.value} onclick={() => appearance.setUiScale(scale.value)} class={`rounded-[7px] px-2.5 py-1 text-xs leading-4 focus-visible:outline-2 focus-visible:outline-ring ${appearance.uiScale === scale.value ? 'bg-chrome-selected text-accent-foreground' : 'text-muted-foreground'}`}>{scale.label}</button>
				{/each}
			</div>
		</div>
	</section>
	<section aria-labelledby="theme-colors" class="space-y-2">
		<h2 id="theme-colors" class="text-body font-medium">{localeState.messages.settings.flow.themeColors}</h2>
		<div class="divide-y divide-border rounded-lg bg-card px-3.5">
			<div class="flex min-h-10 items-center justify-between gap-4 py-2">
				<label for="theme-preset" class="text-body">{localeState.messages.settings.flow.theme}</label>
				<select id="theme-preset" value={appearance.colors.theme} onchange={(event) => appearance.setTheme(event.currentTarget.value as ThemePreferences['theme'])} class="h-7 rounded-md border border-border bg-main-pane px-2 text-xs text-foreground focus-visible:outline-2 focus-visible:outline-ring">{#each themePresets as preset}<option value={preset.id}>{themePresetName(preset.id, localeState.current)}</option>{/each}<option value="custom">{localeState.messages.settings.flow.custom}</option></select>
			</div>
			{#if appearance.colors.theme === 'custom'}
				<div class="flex min-h-10 flex-wrap items-center justify-between gap-2 py-2">
					<span class="text-body">{localeState.messages.settings.flow.editColors}</span>
					<div class="inline-flex rounded-[10px] bg-chrome-hover p-0.75" role="group" aria-label={localeState.messages.settings.flow.editMode}>
						{#each ['light', 'dark'] as mode}<button type="button" aria-pressed={colorMode === mode} onclick={() => { editingMode = mode as ColorMode; editing = true; }} class={`rounded-[7px] px-2 py-0.5 text-xs leading-4 focus-visible:outline-2 focus-visible:outline-ring ${colorMode === mode ? 'bg-chrome-selected text-accent-foreground' : 'text-muted-foreground'}`}>{mode === 'light' ? localeState.messages.settings.flow.light : localeState.messages.settings.flow.dark}</button>{/each}
					</div>
				</div>
				{#each [{ key: 'background', label: localeState.messages.settings.flow.background }, { key: 'card', label: localeState.messages.settings.flow.card }, { key: 'foreground', label: localeState.messages.settings.flow.foreground }] as field}
					<div class="flex min-h-10 items-center justify-between gap-4 py-2"><span class="text-body">{field.label}</span><ColorControl label={field.label} value={appearance.colors[colorMode][field.key as keyof Palette]} onChange={(value) => appearance.setColor(colorMode, field.key as keyof Palette, value)} /></div>
				{/each}
				<div class="flex min-h-10 flex-wrap items-center justify-between gap-3 py-2">
					<span class="text-body">{localeState.messages.settings.flow.accent}</span>
					<div class="flex flex-wrap items-center justify-end gap-2">
						<div class="flex gap-1.5" role="group" aria-label={localeState.messages.settings.flow.accentSwatches}>{#each swatches as color}<button type="button" aria-label={localeState.messages.settings.accentLabel(color)} aria-pressed={appearance.colors[colorMode].accent === color} onclick={() => appearance.setColor(colorMode, 'accent', color)} class={`size-3.5 rounded-full border border-border focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring ${appearance.colors[colorMode].accent === color ? 'ring-1 ring-ring ring-offset-2 ring-offset-card' : ''}`} style:background-color={color}></button>{/each}</div>
						<ColorControl label={localeState.messages.settings.flow.accent} value={appearance.colors[colorMode].accent} onChange={(value) => appearance.setColor(colorMode, 'accent', value)} />
					</div>
				</div>
			{:else}
				<div class="flex min-h-10 items-center justify-between gap-4 py-2">
					<span class="text-body">{localeState.messages.settings.flow.palette}</span>
					<div class="flex items-center gap-1.5" aria-label={localeState.messages.settings.paletteLabel(appearance.resolvedMode === 'dark')}>
						{#each [{ color: presetPalette.background, label: localeState.messages.settings.flow.background }, { color: presetPalette.card, label: localeState.messages.settings.flow.card }, { color: presetPalette.foreground, label: localeState.messages.settings.flow.foreground }, { color: presetPalette.accent, label: localeState.messages.settings.flow.accent }] as swatch}<span class="size-3.5 rounded-full border border-border" style:background-color={swatch.color} title={`${swatch.label} ${swatch.color}`}></span>{/each}
					</div>
				</div>
			{/if}
			<div class="flex min-h-10 items-center justify-between gap-4 py-2"><div><span class="text-body">{localeState.messages.settings.flow.windowMaterial}</span></div><span class="shrink-0 text-xs text-muted-foreground">{localeState.messages.settings.flow.frostedGlass}</span></div>
		</div>
		<div class="flex flex-wrap items-center justify-between gap-3 pt-2">
			<p class="text-xs text-muted-foreground">{appearance.colors.theme === 'custom' ? localeState.messages.settings.flow.customHint : localeState.messages.settings.flow.presetHint}</p>
			<button type="button" onclick={() => appearance.colors.theme === 'custom' ? appearance.resetColors() : appearance.customizeTheme()} class="shrink-0 rounded px-1.5 py-1 text-xs text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring">{appearance.colors.theme === 'custom' ? localeState.messages.settings.flow.resetColors : localeState.messages.settings.flow.customize}</button>
		</div>
		{#if appearance.saveError}<p role="alert" class="text-xs text-destructive">{appearance.saveError}</p>{/if}
	</section>
</div>
