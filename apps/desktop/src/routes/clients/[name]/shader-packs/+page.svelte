<script lang="ts">
	import { untrack } from 'svelte';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { Sparkles, FileArchive, LoaderCircle, Plus, RefreshCw, Search, Trash2, X } from '@lucide/svelte';
	import { ClientShaderPackState } from '#lib/client-shader-pack-state.svelte.js';
	import { localPackSize } from '#lib/local-pack.js';
	import { shaderEnvironmentLabels } from '#lib/shader-packs.js';
	import { getResourceInstallState } from '#lib/resource-install-state.svelte.js';
	import { getClientLaunchState, launchActive } from '#lib/client-launch-state.svelte.js';
	import ModVersionInstaller from '#lib/components/mods/mod-version-installer.svelte';
	import { getClientDetailState } from '#lib/client-detail-state.svelte.js';
	const detail = getClientDetailState();
	let installerOpen = $state(false);
	let installerFooter = $state<HTMLElement>();
	const packs = new ClientShaderPackState();
	const launches = getClientLaunchState();
	const resources = getResourceInstallState();
	let trackedTask: number | null = null;
	const name = $derived(page.params.name ?? '');
	const running = $derived(launchActive(launches.forClient(name)));
	const installing = $derived((resources.active && resources.operation?.name.toLowerCase() === name.toLowerCase()) || (resources.starting && resources.intent?.name.toLowerCase() === name.toLowerCase()));
	const locked = $derived(packs.busy || packs.blocked || running || installing || installerOpen);
	const modsHref = $derived(resolve('/clients/[name]/mods', { name }));
	const environmentMods = $derived([{ label: 'Iris', mod: packs.environment?.iris }, { label: 'Sodium', mod: packs.environment?.sodium }]);
	const applied = $derived(packs.collection !== null && packs.collection.appliedRevision === packs.collection.revision);
	let search = $state('');
	let input: HTMLInputElement;
	const visible = $derived(packs.collection?.packs.filter(pack => pack.filename.toLowerCase().includes(search.trim().toLowerCase())) ?? []);
	const secondary = 'inline-flex h-7 shrink-0 items-center justify-center gap-1.5 rounded-lg px-2 text-xs text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40';
	$effect(() => {
		const target = name;
		untrack(() => { search = ''; installerOpen = false; trackedTask = null; packs.open(target); });
		return () => packs.dispose();
	});
	$effect(() => {
		const task = resources.operation, target = name;
		untrack(() => {
			if (!task || task.name.toLowerCase() !== target.toLowerCase()) return;
			if (task.state === 'running') trackedTask = task.id;
			else if (trackedTask === task.id) { trackedTask = null; packs.queueRefresh(); }
		});
	});
	function pick(event: Event) {
		const element = event.currentTarget as HTMLInputElement, file = element.files?.[0];
		element.value = '';
		if (file && !locked) packs.pick(file);
	}
</script>

<svelte:head><title>{name} · 光影 · Anvil</title></svelte:head>
<section aria-labelledby="shader-packs-title" class="flex min-h-0 flex-1 flex-col overflow-y-auto overscroll-contain px-5 py-5 lg:px-6">
	<header class="flex shrink-0 flex-wrap items-center justify-between gap-2">
		<div class="flex items-baseline gap-2"><h1 id="shader-packs-title" class="text-base font-semibold tracking-tight">光影</h1>{#if packs.collection}<span class="text-[11px] text-muted-foreground">{packs.collection.packs.length} 个</span>{/if}</div>
		<div class="flex items-center gap-1">
			{#if packs.collection?.packs.length}<button type="button" class={secondary} disabled={locked} onclick={() => void packs.sync()} title="校验并补回本实例受管资源文件">{packs.activity === 'syncing' ? '同步中…' : applied ? '同步文件' : '应用到目录'}</button>{/if}
			<button type="button" class={`${secondary} w-7 px-0`} disabled={packs.busy} onclick={() => void packs.refresh()} aria-label="刷新光影清单" title="刷新清单"><RefreshCw size={14} class={packs.busy ? 'animate-spin motion-reduce:animate-none' : ''} aria-hidden="true" /></button>
			<button type="button" disabled={locked || !packs.collection} onclick={() => input.click()} class="inline-flex h-7 items-center gap-1.5 rounded-lg bg-primary px-2.5 text-xs font-medium text-primary-foreground focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><Plus size={13} aria-hidden="true" />导入光影</button>
		</div>
	</header>
	<input bind:this={input} type="file" accept=".zip,application/zip" onchange={pick} aria-label="选择本地光影 ZIP" class="hidden" />
	<p class="mt-2 text-[11px] leading-5 text-muted-foreground">支持本地光影 ZIP，最大 64 MiB。添加后在游戏内选择和调整效果；需要对应的光影加载支持。</p>
	<aside aria-labelledby="shader-environment-title" class="mt-3 rounded-lg bg-card px-3 py-2.5">
		<div class="flex flex-wrap items-center justify-between gap-2">
			<h2 id="shader-environment-title" class="text-xs font-medium">{packs.environment ? shaderEnvironmentLabels[packs.environment.status] : packs.busy ? '正在读取光影环境…' : '光影环境尚未确认'}</h2>
			{#if packs.environment?.status === 'requires_fabric'}<a href={resolve('/clients')} class="rounded text-[11px] text-primary underline-offset-2 hover:underline focus-visible:outline-2 focus-visible:outline-ring">前往客户端</a>
			{:else if packs.environment?.status === 'missing_iris'}<button type="button" disabled={packs.busy || packs.blocked || running || installing || resources.starting} onclick={() => installerOpen = !installerOpen} class="rounded text-[11px] text-primary underline-offset-2 hover:underline focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40">{installerOpen ? '收起安装' : '安装光影支持'}</button>
			{:else if packs.environment}<a href={modsHref} class="rounded text-[11px] text-primary underline-offset-2 hover:underline focus-visible:outline-2 focus-visible:outline-ring">管理模组</a>{/if}
		</div>
		{#if packs.environment}
			{#if packs.environment.iris || packs.environment.sodium}<div class="mt-2 flex flex-wrap gap-1.5 text-[11px] text-muted-foreground">{#each environmentMods as { label, mod }}{#if mod}<span class="max-w-full break-all rounded bg-chrome-hover px-1.5 py-0.5">{label} {mod.version}{mod.enabled ? '' : ' · 未启用'}</span>{/if}{/each}</div>{/if}
			<p class="mt-1.5 wrap-break-word text-[11px] leading-5 text-muted-foreground">{packs.environment.reason}</p>
			<p class="mt-1 text-[10px] text-muted-foreground">仅识别 Anvil 管理的模组；添加光影不会自动在游戏中启用。</p>
		{:else if !packs.busy}<p class="mt-1.5 text-[11px] leading-5 text-muted-foreground">请刷新以读取环境状态。已添加的光影文件仍保留。</p>{/if}
	</aside>
	{#if installerOpen && detail.client}
		<section aria-label="安装 Iris 光影支持" class="mt-3 shrink-0 overflow-hidden rounded-lg bg-card">
			<div class="flex items-center justify-between px-3 pt-3"><h2 class="text-xs font-medium">Iris · Minecraft {detail.client.versionId}</h2><button type="button" class={secondary} onclick={() => installerOpen = false} aria-label="关闭 Iris 安装"><X size={13} aria-hidden="true" /></button></div>
			<p class="px-3 pt-1 text-[11px] leading-5 text-muted-foreground">选择兼容版本，再确认必需依赖及下载量。安装任务离开页面后仍继续。无法确认的模组冲突会拒绝安装。</p>
			<div class="flex h-48 min-h-0 flex-col px-3 pb-2"><ModVersionInstaller source="modrinth" {name} gameVersion={detail.client.versionId} project="YL57xq9U" footerTarget={installerFooter} blocked={packs.busy || packs.blocked || running} /></div>
			<div bind:this={installerFooter}></div>
		</section>
	{/if}
	{#if installing}<p role="status" class="mt-2 text-[11px] text-muted-foreground">当前实例的模组任务进行中，完成后会刷新光影环境。</p>{/if}
	{#if running}<p role="status" class="mt-2 text-[11px] text-muted-foreground">游戏运行或准备时无法修改光影。</p>{/if}
	{#if packs.error}<div role="alert" class="mt-3 rounded-lg bg-destructive/5 px-3 py-2 text-xs text-destructive"><p class="wrap-break-word">{packs.error}</p><button type="button" class="mt-1 rounded px-1 py-1 text-xs text-primary focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40" disabled={packs.busy} onclick={() => void packs.refresh()}>{packs.blocked ? '刷新确认结果' : '重新读取'}</button></div>{/if}
	{#if packs.message}<p role="status" class="mt-3 text-[11px] text-muted-foreground">{packs.message}</p>{/if}
	{#if packs.selected}
		<div class="mt-3 rounded-lg border border-primary/25 bg-card p-3">
			<div class="flex items-center gap-2"><FileArchive size={16} class="shrink-0 text-primary" aria-hidden="true" /><span class="min-w-0 flex-1 truncate text-[13px]" title={packs.selected.name}>{packs.selected.name}</span><span class="shrink-0 text-[11px] text-muted-foreground">{localPackSize(packs.selected.size)}</span></div>
			<p class="mt-2 text-[11px] leading-5 text-muted-foreground">确认后校验 ZIP 并添加到此实例，相同内容复用共享原件。游戏内不会自动启用。</p>
			<div class="mt-2 flex justify-end gap-1"><button type="button" class={secondary} disabled={packs.busy} onclick={() => packs.selected = null}>取消</button><button type="button" disabled={locked} onclick={() => void packs.importSelected()} class="inline-flex h-7 items-center gap-1.5 rounded-lg bg-primary px-3 text-xs text-primary-foreground focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40">{#if packs.activity === 'importing'}<LoaderCircle size={13} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />导入中…{:else}确认导入{/if}</button></div>
		</div>
	{/if}
	{#if !packs.collection && packs.busy}
		<div role="status" class="flex min-h-40 flex-1 items-center justify-center gap-2 text-xs text-muted-foreground"><LoaderCircle size={15} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />读取光影…</div>
	{:else if packs.collection}
		{#if packs.collection.packs.length === 0}
			<div class="flex min-h-52 flex-1 flex-col items-center justify-center gap-2.5 text-center"><Sparkles size={26} strokeWidth={1.4} class="text-primary/60" aria-hidden="true" /><p class="text-[13px]">还没有光影</p><p class="text-[11px] text-muted-foreground">导入本地光影 ZIP，再到游戏中选择效果。</p></div>
		{:else}
			<label class="mt-4 flex h-8 items-center gap-2 rounded-lg bg-chrome-hover px-2.5 text-muted-foreground focus-within:ring-1 focus-within:ring-ring"><Search size={14} aria-hidden="true" /><input type="search" bind:value={search} placeholder="搜索光影名称" aria-label="搜索光影" class="min-w-0 flex-1 bg-transparent text-xs text-foreground outline-none placeholder:text-muted-foreground" />{#if search}<button type="button" onclick={() => search = ''} aria-label="清空搜索" class="rounded p-0.5 focus-visible:outline-2 focus-visible:outline-ring"><X size={12} aria-hidden="true" /></button>{/if}</label>
			<ul aria-label="本实例光影" aria-busy={packs.busy} class="mt-3 shrink-0 divide-y divide-border rounded-lg bg-card px-3">
				{#each visible as pack (pack.sha256)}
					<li class="py-3">
						<div class="flex items-start gap-2.5"><div class="mt-0.5 grid size-8 shrink-0 place-items-center rounded-lg bg-primary/8 text-primary"><FileArchive size={17} strokeWidth={1.5} aria-hidden="true" /></div><div class="min-w-0 flex-1"><h2 class="truncate text-[13px] font-medium" title={pack.filename}>{pack.filename}</h2><p class="mt-1.5 text-[11px] text-muted-foreground">{localPackSize(pack.size)} · 兼容性未确认 · {applied ? '已添加到实例' : '待同步到目录'}</p></div><button type="button" class={`${secondary} w-7 px-0 hover:text-destructive`} disabled={locked} aria-label={`移除 ${pack.filename}`} title="从此实例移除" onclick={() => { packs.selected = null; packs.removing = pack.sha256; }}><Trash2 size={13} aria-hidden="true" /></button></div>
						{#if packs.removing === pack.sha256}<div class="mt-2 flex flex-wrap items-center justify-end gap-1 rounded-lg bg-chrome-hover px-2 py-2"><p class="mr-auto text-[11px] text-muted-foreground">移除此实例的 ZIP 副本，共享原件保留。</p><button type="button" class={secondary} disabled={packs.busy} onclick={() => packs.removing = null}>取消</button><button type="button" disabled={locked} onclick={() => void packs.remove(pack)} class="h-7 rounded-lg px-2 text-xs text-destructive focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40">{packs.activity === 'removing' ? '移除中…' : '确认移除'}</button></div>{/if}
					</li>
				{/each}
			</ul>
			{#if visible.length === 0}<p role="status" class="py-6 text-center text-xs text-muted-foreground">没有匹配的光影</p>{/if}
		{/if}
	{/if}
</section>
