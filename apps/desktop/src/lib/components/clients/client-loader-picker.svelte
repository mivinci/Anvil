<script module lang="ts">
	export type ClientLoader = 'vanilla' | 'fabric' | 'neoforge' | 'forge';
</script>

<script lang="ts">
	import { listFabricVersions, type FabricLoaderVersion } from '../../runtime';
	let { value = $bindable<ClientLoader>('vanilla'), gameVersion = null, version = $bindable<string | null>(null) }: { value?: ClientLoader; gameVersion?: string | null; version?: string | null } = $props();
	let versions = $state<FabricLoaderVersion[]>([]);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let retry = $state(0);
	$effect(() => {
		const game = gameVersion;
		const selectedLoader = value;
		void retry;
		version = null; versions = []; error = null; loading = false;
		if (selectedLoader !== 'fabric' || !game) return;
		const controller = new AbortController();
		loading = true;
		void (async () => {
			try {
				const result = await listFabricVersions(game, AbortSignal.any([controller.signal, AbortSignal.timeout(30_000)]));
				if (controller.signal.aborted) return;
				if (!Array.isArray(result) || result.length > 1024 || result.some((item) => !item || typeof item.version !== 'string' || typeof item.stable !== 'boolean')) throw new Error('Fabric 版本目录格式不正确。');
				versions = result; version = result.find((item) => item.stable)?.version ?? result[0]?.version ?? null;
			} catch { if (!controller.signal.aborted) error = '暂时无法读取 Fabric 版本。'; }
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

<section aria-labelledby="client-loader-title" class="mt-4">
	<h3 id="client-loader-title" class="text-[13px] font-medium">加载器</h3>
	<div role="group" aria-label="选择模组加载器" class="mt-2 grid grid-cols-4 gap-2">
		{#each options as option (option.value)}
			<button type="button" aria-pressed={value === option.value} onclick={() => value = option.value} class={`min-w-0 rounded-lg border px-2 py-2 text-left transition-colors focus-visible:outline-2 focus-visible:outline-ring ${value === option.value ? 'border-primary/40 bg-chrome-selected' : 'border-border bg-card hover:bg-chrome-hover'}`}><span class={`block truncate text-xs font-medium ${value === option.value ? 'text-accent-foreground' : 'text-foreground'}`}>{option.label}</span><span class="mt-1 block truncate text-[10px] text-muted-foreground">{option.hint}</span></button>
		{/each}
	</div>
	{#if value === 'fabric'}
		<div class="mt-2 flex items-center justify-between gap-3 rounded-lg border border-border px-3 py-2">
			<label for="fabric-loader-version" class="text-xs">Fabric 版本</label>
			<select id="fabric-loader-version" bind:value={version} disabled={loading || versions.length === 0} class="h-7 max-w-52 rounded-md border border-input bg-popover px-2 text-xs text-foreground outline-none focus:ring-1 focus:ring-ring disabled:text-muted-foreground">
				{#if !version}<option value={null}>{loading ? '读取中…' : !gameVersion ? '请先选择游戏版本' : error ? '读取失败' : '没有可用版本'}</option>{/if}
				{#each versions as item (item.version)}<option value={item.version}>{item.version}{item.stable ? ' · 稳定版' : ' · 预览版'}</option>{/each}
			</select>
		</div>
		{#if error}<p role="alert" class="mt-2 text-[11px] text-destructive">{error}<button type="button" onclick={() => retry++} class="ml-1 underline underline-offset-2">重试</button></p>
		{:else if !loading && gameVersion && versions.length === 0}<p class="mt-2 text-[11px] text-muted-foreground">Fabric 暂未提供此游戏版本的加载器。</p>{/if}
	{:else if value !== 'vanilla'}
		<div class="mt-2 flex items-center justify-between gap-3 rounded-lg border border-border px-3 py-2"><span class="text-xs">{selected.label} 版本</span><span aria-disabled="true" class="text-[11px] text-muted-foreground">尚未开放安装</span></div>
	{:else}
		<p class="mt-2 text-[11px] leading-4 text-muted-foreground">直接安装原版游戏，无需选择加载器版本。</p>
	{/if}
</section>
