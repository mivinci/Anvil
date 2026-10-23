<script module lang="ts">
	export type ClientLoader = 'vanilla' | 'fabric' | 'neoforge' | 'forge';
</script>

<script lang="ts">
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
					if (!Array.isArray(result.versions) || result.versions.length > 1024 || result.versions.some(item => !item || typeof item.version !== 'string' || typeof item.stable !== 'boolean')) throw new Error('Fabric 版本目录格式不正确。');
					versions = result.versions;
					if (!versions.some(item => item.version === version)) version = versions.find(item => item.stable)?.version ?? versions[0]?.version ?? null;
					error = result.refreshError;
					if (!result.stale || result.refreshError || force) break;
					options = { refresh: true, after: result.revision };
				}
			} catch { if (!controller.signal.aborted) error = '暂时无法更新 Fabric 版本。'; }
			finally { if (!controller.signal.aborted) loading = false; }
		})();
		return () => controller.abort();
	});
	const options: { value: ClientLoader; label: string; hint: string }[] = [
		{ value: 'vanilla', label: '原版', hint: '不使用加载器' },
		{ value: 'fabric', label: 'Fabric', hint: '模组加载器' },
		{ value: 'neoforge', label: 'NeoForge', hint: '模组加载器' },
		{ value: 'forge', label: 'Forge', hint: '模组加载器' },
	];
	const selected = $derived(options.find((option) => option.value === value)!);
</script>

<section aria-labelledby="client-loader-title" class="mt-4 flex min-h-0 flex-1 flex-col">
	<h3 id="client-loader-title" class="shrink-0 text-[13px] font-medium">加载器</h3>
	<div role="group" aria-label="选择模组加载器" class="mt-2 grid shrink-0 grid-cols-4 gap-2">
		{#each options as option (option.value)}
			<button type="button" aria-pressed={value === option.value} onclick={() => value = option.value} class={`min-w-0 rounded-lg border px-2 py-2 text-left transition-colors focus-visible:outline-2 focus-visible:outline-ring ${value === option.value ? 'border-primary/40 bg-chrome-selected' : 'border-border bg-card hover:bg-chrome-hover'}`}><span class={`block truncate text-xs font-medium ${value === option.value ? 'text-accent-foreground' : 'text-foreground'}`}>{option.label}</span><span class="mt-1 block truncate text-[10px] text-muted-foreground">{option.hint}</span></button>
		{/each}
	</div>
	{#if value === 'fabric'}
		<div class="mt-3 flex shrink-0 items-center justify-between gap-3">
			<h4 id="fabric-version-title" class="text-[12px] font-medium">Fabric 版本</h4>
			<div class="flex items-center gap-1"><button type="button" onclick={refresh} disabled={loading || !gameVersion} aria-label="刷新 Fabric 版本" class="grid size-7 place-items-center rounded-lg text-muted-foreground hover:bg-chrome-hover disabled:opacity-40"><RefreshCw size={13} class={loading ? 'animate-spin motion-reduce:animate-none' : ''} /></button><label class="flex h-7 w-40 items-center gap-1.5 rounded-lg bg-chrome-hover px-2 text-muted-foreground focus-within:ring-1 focus-within:ring-ring"><Search size={13} aria-hidden="true" /><input type="search" aria-label="搜索 Fabric 版本" placeholder="搜索版本号" bind:value={query} class="min-w-0 flex-1 bg-transparent text-[12px] text-foreground outline-none placeholder:text-muted-foreground" /></label></div>
		</div>
		<div aria-busy={loading} class="mt-2 min-h-20 flex-1 overflow-y-auto overscroll-contain rounded-lg border border-border">
			{#if loading && versions.length === 0}<div role="status" class="flex h-full items-center justify-center gap-2 p-3 text-[12px] text-muted-foreground"><LoaderCircle size={14} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />正在读取 Fabric 版本…</div>
			{:else if error && versions.length === 0}<div role="alert" class="p-3 text-[12px] text-destructive">{error}<button type="button" onclick={refresh} class="ml-2 rounded px-1 text-primary hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring">重试</button></div>
			{:else if visible.length === 0}<p role="status" class="flex h-full items-center justify-center p-3 text-[12px] text-muted-foreground">{!gameVersion ? '请先选择游戏版本' : versions.length ? '没有匹配的 Fabric 版本' : 'Fabric 暂未提供此游戏版本的加载器'}</p>
			{:else}
				<SelectableList role="group" aria-labelledby="fabric-version-title">
					{#each visible as item (item.version)}
						<SelectableRow selected={version === item.version} aria-pressed={version === item.version} onclick={() => version = item.version}>
							<span class="truncate">{item.version}</span>
							{#snippet trailing()}{item.stable ? '稳定版' : '预览版'}{/snippet}
						</SelectableRow>
					{/each}
				</SelectableList>
			{/if}
		</div>
		{#if error && versions.length > 0}<p role="status" class="mt-1 text-[11px] text-muted-foreground">{error} 已保留现有目录。</p>{/if}
	{:else if value !== 'vanilla'}
		<div class="mt-2 flex items-center justify-between gap-3 rounded-lg border border-border px-3 py-2"><span class="text-xs">{selected.label} 版本</span><span aria-disabled="true" class="text-[11px] text-muted-foreground">尚未开放安装</span></div>
	{:else}
		<p class="mt-2 text-[11px] leading-4 text-muted-foreground">直接安装原版游戏，无需选择加载器版本。</p>
	{/if}
</section>
