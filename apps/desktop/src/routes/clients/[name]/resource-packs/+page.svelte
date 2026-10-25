<script lang="ts">
	import { untrack } from 'svelte';
	import { page } from '$app/state';
	import { Image, FileArchive, LoaderCircle, Plus, RefreshCw, Search, Trash2, X } from '@lucide/svelte';
	import { ClientResourcePackState, resourcePackFormat, resourcePackSize } from '#lib/client-resource-pack-state.svelte.js';
	import { getClientLaunchState, launchActive } from '#lib/client-launch-state.svelte.js';
	const packs = new ClientResourcePackState();
	const launches = getClientLaunchState();
	const name = $derived(page.params.name ?? '');
	const running = $derived(launchActive(launches.forClient(name)));
	const locked = $derived(packs.busy || packs.uncertain || running);
	const applied = $derived(packs.collection !== null && packs.collection.appliedRevision === packs.collection.revision);
	let search = $state('');
	let input: HTMLInputElement;
	const visible = $derived(packs.collection?.packs.filter(pack => `${pack.filename} ${pack.metadata.description}`.toLowerCase().includes(search.trim().toLowerCase())) ?? []);
	const secondary = 'inline-flex h-7 shrink-0 items-center justify-center gap-1.5 rounded-lg px-2 text-xs text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40';
	$effect(() => {
		const target = name;
		untrack(() => { search = ''; packs.open(target); });
		return () => packs.dispose();
	});
	function pick(event: Event) {
		const element = event.currentTarget as HTMLInputElement, file = element.files?.[0];
		element.value = '';
		if (file && !running) packs.pick(file);
	}
</script>

<svelte:head><title>{name} · 资源包 · Anvil</title></svelte:head>
<section aria-labelledby="resource-packs-title" class="flex min-h-0 flex-1 flex-col overflow-y-auto overscroll-contain px-5 py-5 lg:px-6">
	<header class="flex shrink-0 flex-wrap items-center justify-between gap-2">
		<div class="flex items-baseline gap-2"><h1 id="resource-packs-title" class="text-base font-semibold tracking-tight">资源包</h1>{#if packs.collection}<span class="text-[11px] text-muted-foreground">{packs.collection.packs.length} 个</span>{/if}</div>
		<div class="flex items-center gap-1">
			{#if packs.collection?.packs.length}<button type="button" class={secondary} disabled={locked} onclick={() => void packs.sync()} title="校验并补回本实例受管资源文件">{packs.activity === 'syncing' ? '同步中…' : applied ? '同步文件' : '应用到目录'}</button>{/if}
			<button type="button" class={`${secondary} w-7 px-0`} disabled={packs.busy} onclick={() => void packs.refresh()} aria-label="刷新资源包清单" title="刷新清单"><RefreshCw size={14} class={packs.busy ? 'animate-spin motion-reduce:animate-none' : ''} aria-hidden="true" /></button>
			<button type="button" disabled={locked || !packs.collection} onclick={() => input.click()} class="inline-flex h-7 items-center gap-1.5 rounded-lg bg-primary px-2.5 text-xs font-medium text-primary-foreground focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><Plus size={13} aria-hidden="true" />导入资源包</button>
		</div>
	</header>
	<input bind:this={input} type="file" accept=".zip,application/zip" onchange={pick} aria-label="选择本地资源包 ZIP" class="hidden" />
	<p class="mt-2 text-[11px] leading-5 text-muted-foreground">添加到此实例后，在游戏内选择并调整顺序。支持本地 ZIP，最大 64 MiB；版本兼容性由游戏确认。</p>
	{#if running}<p role="status" class="mt-2 text-[11px] text-muted-foreground">游戏运行或准备时无法修改资源包。</p>{/if}
	{#if packs.error}<div role="alert" class="mt-3 rounded-lg bg-destructive/5 px-3 py-2 text-xs text-destructive"><p class="wrap-break-word">{packs.error}</p><button type="button" class="mt-1 rounded px-1 py-1 text-xs text-primary focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40" disabled={packs.busy} onclick={() => void packs.refresh()}>{packs.uncertain ? '刷新确认结果' : '重新读取'}</button></div>{/if}
	{#if packs.message}<p role="status" class="mt-3 text-[11px] text-muted-foreground">{packs.message}</p>{/if}
	{#if packs.selected}
		<div class="mt-3 rounded-lg border border-primary/25 bg-card p-3">
			<div class="flex items-center gap-2"><FileArchive size={16} class="shrink-0 text-primary" aria-hidden="true" /><span class="min-w-0 flex-1 truncate text-[13px]" title={packs.selected.name}>{packs.selected.name}</span><span class="shrink-0 text-[11px] text-muted-foreground">{resourcePackSize(packs.selected.size)}</span></div>
			<p class="mt-2 text-[11px] leading-5 text-muted-foreground">确认后校验 ZIP 并添加到此实例，相同内容复用共享原件。游戏内不会自动启用。</p>
			<div class="mt-2 flex justify-end gap-1"><button type="button" class={secondary} disabled={packs.busy} onclick={() => packs.selected = null}>取消</button><button type="button" disabled={locked} onclick={() => void packs.importSelected()} class="inline-flex h-7 items-center gap-1.5 rounded-lg bg-primary px-3 text-xs text-primary-foreground focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40">{#if packs.activity === 'importing'}<LoaderCircle size={13} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />导入中…{:else}确认导入{/if}</button></div>
		</div>
	{/if}
	{#if !packs.collection && packs.busy}
		<div role="status" class="flex min-h-40 flex-1 items-center justify-center gap-2 text-xs text-muted-foreground"><LoaderCircle size={15} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />读取资源包…</div>
	{:else if packs.collection}
		{#if packs.collection.packs.length === 0}
			<div class="mt-3 flex flex-wrap items-center gap-3 rounded-lg bg-card px-3.5 py-3">
				<span class="grid size-9 shrink-0 place-items-center rounded-lg bg-primary/8 text-primary"><Image size={18} strokeWidth={1.5} aria-hidden="true" /></span>
				<div class="min-w-0 flex-1"><p class="text-[13px] font-medium">还没有资源包</p><p class="mt-1 text-[11px] leading-4 text-muted-foreground">导入一个 ZIP，为这个实例换一套纹理。</p></div>
				<button type="button" disabled={locked} onclick={() => input.click()} class="inline-flex h-8 shrink-0 items-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><Plus size={13} aria-hidden="true" />导入资源包</button>
			</div>
		{:else}
			<label class="mt-4 flex h-8 items-center gap-2 rounded-lg bg-chrome-hover px-2.5 text-muted-foreground focus-within:ring-1 focus-within:ring-ring"><Search size={14} aria-hidden="true" /><input type="search" bind:value={search} placeholder="搜索名称或描述" aria-label="搜索资源包" class="min-w-0 flex-1 bg-transparent text-xs text-foreground outline-none placeholder:text-muted-foreground" />{#if search}<button type="button" onclick={() => search = ''} aria-label="清空搜索" class="rounded p-0.5 focus-visible:outline-2 focus-visible:outline-ring"><X size={12} aria-hidden="true" /></button>{/if}</label>
			<ul aria-label="本实例资源包" aria-busy={packs.busy} class="mt-3 shrink-0 divide-y divide-border rounded-lg bg-card px-3">
				{#each visible as pack (pack.sha256)}
					<li class="py-3">
						<div class="flex items-start gap-2.5"><div class="mt-0.5 grid size-8 shrink-0 place-items-center rounded-lg bg-primary/8 text-primary"><FileArchive size={17} strokeWidth={1.5} aria-hidden="true" /></div><div class="min-w-0 flex-1"><h2 class="truncate text-[13px] font-medium" title={pack.filename}>{pack.filename}</h2>{#if pack.metadata.description}<p class="mt-1 line-clamp-2 wrap-break-word text-[11px] leading-4 text-muted-foreground">{pack.metadata.description}</p>{/if}<p class="mt-1.5 text-[11px] text-muted-foreground">{resourcePackSize(pack.size)} · 格式 {resourcePackFormat(pack)} · {applied ? '已添加到实例' : '待同步到目录'}</p></div><button type="button" class={`${secondary} w-7 px-0 hover:text-destructive`} disabled={locked} aria-label={`移除 ${pack.filename}`} title="从此实例移除" onclick={() => { packs.selected = null; packs.removing = pack.sha256; }}><Trash2 size={13} aria-hidden="true" /></button></div>
						{#if packs.removing === pack.sha256}<div class="mt-2 flex flex-wrap items-center justify-end gap-1 rounded-lg bg-chrome-hover px-2 py-2"><p class="mr-auto text-[11px] text-muted-foreground">移除此实例的 ZIP 副本，共享原件保留。</p><button type="button" class={secondary} disabled={packs.busy} onclick={() => packs.removing = null}>取消</button><button type="button" disabled={locked} onclick={() => void packs.remove(pack)} class="h-7 rounded-lg px-2 text-xs text-destructive focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40">{packs.activity === 'removing' ? '移除中…' : '确认移除'}</button></div>{/if}
					</li>
				{/each}
			</ul>
			{#if visible.length === 0}<p role="status" class="py-6 text-center text-xs text-muted-foreground">没有匹配的资源包</p>{/if}
		{/if}
	{/if}
</section>
