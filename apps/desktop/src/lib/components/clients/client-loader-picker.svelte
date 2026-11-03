<script module lang="ts">
	export type ClientLoader = 'vanilla' | 'fabric' | 'neoforge' | 'forge';
</script>

<script lang="ts">
	import { localeState } from '#lib/locale.svelte.ts';
	import { untrack } from 'svelte';
	import { Search, LoaderCircle, RefreshCw } from '@lucide/svelte';
	import SelectableList from '../ui/selectable-list.svelte';
	import SelectableRow from '../ui/selectable-row.svelte';
	import { listFabricVersions, type CatalogOptions, type FabricLoaderVersion } from '../../runtime';
	let { value = $bindable<ClientLoader>('vanilla'), gameVersion = null, version = $bindable<string | null>(null) }: { value?: ClientLoader; gameVersion?: string | null; version?: string | null } = $props();
	let versions = $state<FabricLoaderVersion[]>([]);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let retry = $state(0);
	let query = $state('');
	const visible = $derived(versions.filter(item => item.version.toLowerCase().includes(query.trim().toLowerCase())));
	let forceNext = false;
	let previousGame: string | null = null;
	function refresh() { forceNext = true; retry++; }
	$effect(() => {
		const game = gameVersion, selectedLoader = value;
		void retry;
		const force = forceNext; forceNext = false;
		untrack(() => {
			if (game !== previousGame || selectedLoader !== 'fabric') { version = null; versions = []; query = ''; }
			previousGame = game;
		});
		error = null; loading = false;
		if (selectedLoader !== 'fabric' || !game) return;
		const controller = new AbortController();
		loading = true;
		void (async () => {
			try {
				const signal = AbortSignal.any([controller.signal, AbortSignal.timeout(55_000)]);
				let options: CatalogOptions = { refresh: force };
				for (let attempt = 0; attempt < 2; attempt++) {
					const result = await listFabricVersions(game, signal, options);
					signal.throwIfAborted();
					if (!Array.isArray(result.versions) || result.versions.length > 1024 || result.versions.some(item => !item || typeof item.version !== 'string' || typeof item.stable !== 'boolean')) throw new Error(localeState.messages.clients.flow.invalidFabricCatalog);
					versions = result.versions;
					if (!versions.some(item => item.version === version)) version = versions.find(item => item.stable)?.version ?? versions[0]?.version ?? null;
					error = result.refreshError;
					if (!result.stale || result.refreshError || force) break;
					options = { refresh: true, after: result.revision };
				}
			} catch { if (!controller.signal.aborted) error = localeState.messages.clients.flow.fabricFailed; }
			finally { if (!controller.signal.aborted) loading = false; }
		})();
		return () => controller.abort();
	});
	const options = $derived<{ value: ClientLoader; label: string; hint: string }[]>([
		{ value: 'vanilla', label: localeState.messages.clients.flow.vanilla, hint: localeState.messages.clients.flow.noLoader },
		{ value: 'fabric', label: 'Fabric', hint: localeState.messages.clients.flow.modLoader },
		{ value: 'neoforge', label: 'NeoForge', hint: localeState.messages.clients.flow.modLoader },
		{ value: 'forge', label: 'Forge', hint: localeState.messages.clients.flow.modLoader },
	]);
	const selected = $derived(options.find((option) => option.value === value)!);
</script>

<section aria-labelledby="client-loader-title" class="mt-4 flex min-h-0 flex-1 flex-col">
	<h3 id="client-loader-title" class="shrink-0 text-body font-medium">{localeState.messages.clients.flow.loader}</h3>
	<div role="group" aria-label={localeState.messages.clients.flow.chooseModLoader} class="mt-2 grid shrink-0 grid-cols-4 gap-2">
		{#each options as option (option.value)}
			<button type="button" aria-pressed={value === option.value} onclick={() => value = option.value} class={`min-w-0 rounded-lg border px-2 py-2 text-left transition-colors focus-visible:outline-2 focus-visible:outline-ring ${value === option.value ? 'border-primary/40 bg-chrome-selected' : 'border-border bg-card hover:bg-chrome-hover'}`}><span class={`block truncate text-xs font-medium ${value === option.value ? 'text-accent-foreground' : 'text-foreground'}`}>{option.label}</span><span class="mt-1 block truncate text-xs text-muted-foreground">{option.hint}</span></button>
		{/each}
	</div>
	{#if value === 'fabric'}
		<div class="mt-3 flex shrink-0 items-center justify-between gap-3">
			<h4 id="fabric-version-title" class="text-xs font-medium">{localeState.messages.clients.flow.fabricVersion}</h4>
			<div class="flex items-center gap-1"><button type="button" onclick={refresh} disabled={loading || !gameVersion} aria-label={localeState.messages.clients.flow.refreshFabric} class="grid size-7 place-items-center rounded-lg text-muted-foreground hover:bg-chrome-hover disabled:opacity-40"><RefreshCw size={13} class={loading ? 'animate-spin motion-reduce:animate-none' : ''} /></button><label class="flex h-7 w-40 items-center gap-1.5 rounded-lg bg-chrome-hover px-2 text-muted-foreground focus-within:ring-1 focus-within:ring-ring"><Search size={13} aria-hidden="true" /><input type="search" aria-label={localeState.messages.clients.flow.searchFabric} placeholder={localeState.messages.clients.flow.searchVersion} bind:value={query} class="min-w-0 flex-1 bg-transparent text-xs text-foreground outline-none placeholder:text-muted-foreground" /></label></div>
		</div>
		<div aria-busy={loading} class="mt-2 min-h-20 flex-1 overflow-y-auto overscroll-contain rounded-lg border border-border">
			{#if loading && versions.length === 0}<div role="status" class="flex h-full items-center justify-center gap-2 p-3 text-xs text-muted-foreground"><LoaderCircle size={14} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{localeState.messages.clients.flow.loadingFabric}</div>
			{:else if error && versions.length === 0}<div role="alert" class="p-3 text-xs text-destructive">{localeState.messages.clients.flow.fabricFailed}<button type="button" onclick={refresh} class="ml-2 rounded px-1 text-primary hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring">{localeState.messages.clients.flow.retry}</button></div>
			{:else if visible.length === 0}<p role="status" class="flex h-full items-center justify-center p-3 text-xs text-muted-foreground">{!gameVersion ? localeState.messages.clients.flow.chooseGameFirst : versions.length ? localeState.messages.clients.flow.noFabricMatches : localeState.messages.clients.flow.fabricMissing}</p>
			{:else}
				<SelectableList role="group" aria-labelledby="fabric-version-title">
					{#each visible as item (item.version)}
						<SelectableRow selected={version === item.version} aria-pressed={version === item.version} onclick={() => version = item.version}>
							<span class="truncate">{item.version}</span>
							{#snippet trailing()}{item.stable ? localeState.messages.clients.flow.stable : localeState.messages.clients.flow.preview}{/snippet}
						</SelectableRow>
					{/each}
				</SelectableList>
			{/if}
		</div>
		{#if error && versions.length > 0}<p role="status" class="mt-1 text-xs text-muted-foreground">{localeState.messages.clients.catalogKept}</p>{/if}
		{#if error}<details class="mt-1 text-xs text-muted-foreground"><summary>{localeState.messages.errors.technicalDetails}</summary><p class="whitespace-pre-wrap break-words">{error}</p></details>{/if}
	{:else if value !== 'vanilla'}
		<div class="mt-2 flex items-center justify-between gap-3 rounded-lg border border-border px-3 py-2"><span class="text-xs">{localeState.messages.clients.loaderVersion(selected.label)}</span><span aria-disabled="true" class="text-xs text-muted-foreground">{localeState.messages.clients.flow.loaderUnavailable}</span></div>
	{:else}
		<p class="mt-2 text-xs leading-4 text-muted-foreground">{localeState.messages.clients.flow.vanillaDescription}</p>
	{/if}
</section>
