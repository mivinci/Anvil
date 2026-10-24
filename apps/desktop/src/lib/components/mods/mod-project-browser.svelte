<script lang="ts">
	import { untrack, tick } from 'svelte';
	import { ModProjectFeed, type ModProjectFeedSnapshot } from '../../mod-catalog-read';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { isTauri } from '@tauri-apps/api/core';
	import { ChevronRight, Search, RefreshCw, LoaderCircle } from '@lucide/svelte';
	import { shell } from '../../shell';
	import { ModCatalogState } from '../../mod-catalog-state.svelte';
	import ModProjectIcon from './mod-project-icon.svelte';
	import ModProjectStats from './mod-project-stats.svelte';
	import ModVersionInstaller from './mod-version-installer.svelte';
	import { getCurseForgeConfiguration, searchModProjects, getModProject,
		type ModSource, type ModProjectDetails } from '../../runtime';
	let { source, target, chooseInstance }: { source: ModSource; target: { name: string; version: string } | null; chooseInstance: () => void } = $props();
	let projects = $state<ModProjectFeedSnapshot>({ items: [], nextIndex: null, busy: false, error: null, warning: null, loaded: false });
	const feed = new ModProjectFeed(value => { projects = value; });
	let listElement = $state<HTMLDivElement>();
	let sentinel = $state<HTMLDivElement>();
	let savedScroll = 0;
	let installFooter = $state<HTMLDivElement>();
	const details = new ModCatalogState<ModProjectDetails>();
	const query = $derived(page.url.searchParams.get('q') ?? '');
	const index = $derived(Math.max(0, Number(page.url.searchParams.get('index')) || 0));
	const projectId = $derived(page.url.searchParams.get('project'));
	const validProject = $derived(projectId !== null && (source === 'modrinth' ? /^[a-zA-Z0-9]{8}$/.test(projectId) : /^[1-9][0-9]*$/.test(projectId)));
	let draft = $state('');
	let configured = $state(false);
	let configBusy = $state(false);
	let configError = $state<string | null>(null);
	let configRevision = $state(0);
	let navigationError = $state<string | null>(null);
	const iconButton = 'grid size-9 shrink-0 place-items-center rounded-lg text-muted-foreground transition-colors hover:bg-chrome-hover hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40';
	const button = 'rounded-lg px-2.5 py-1.5 text-xs hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40';
	$effect(() => { draft = query; });
	$effect(() => {
		const current = source; configRevision;
		configured = false; configError = null;
		if (current === 'modrinth') { configured = true; configBusy = false; return; }
		const controller = new AbortController(); configBusy = true;
		void getCurseForgeConfiguration(AbortSignal.any([controller.signal, AbortSignal.timeout(15_000)]))
			.then(value => { if (!controller.signal.aborted) { configured = value.configured; configError = value.configured ? null : value.issue ?? 'CurseForge 尚未配置 API Key，请先在 Runtime 配置。'; } })
			.catch(error => { if (!controller.signal.aborted) configError = String(error); })
			.finally(() => { if (!controller.signal.aborted) configBusy = false; });
		return () => controller.abort();
	});

	function loadDetails(force = false) {
		const current = source, id = projectId!;
		return details.load((signal, options) => getModProject(current, id, signal, options), force);
	}

	$effect(() => {
		source; query; index; target?.name; target?.version; const enabled = configured;
		const current = source, q = query, offset = index, version = target?.version;
		untrack(() => {
			savedScroll = 0;
			if (listElement) listElement.scrollTop = 0;
			feed.reset(enabled ? (cursor, signal, options) => searchModProjects(current, q, cursor, signal, { ...options, loader: 'fabric', ...(version ? { gameVersion: version } : {}) }) : null, offset);
		});
		return () => feed.cancel();
	});
	$effect(() => {
		const element = listElement;
		if (projectId || !element) return;
		let active = true;
		void tick().then(() => { if (active) element.scrollTop = savedScroll; });
		return () => { active = false; };
	});
	$effect(() => {
		const root = listElement, node = sentinel;
		const eligible = configured && !projects.busy && !projects.error && !projects.warning && projects.nextIndex !== null;
		projectId;
		if (!root || !node || !eligible) return;
		const observer = new IntersectionObserver(entries => {
			if (entries.some(entry => entry.isIntersecting) && root.getClientRects().length > 0) void feed.more();
		}, { root, rootMargin: '0px 0px 160px 0px' });
		observer.observe(node);
		return () => observer.disconnect();
	});

	$effect(() => {
		source; projectId; const enabled = configured && validProject;
		untrack(() => { details.clear(); if (enabled) void loadDetails(); });
		return () => details.cancel();
	});

	async function navigate(change: { q?: string; index?: number; project?: string | null }) {
		if (change.project && listElement) savedScroll = listElement.scrollTop;
		const next = new URL(page.url.href);
		for (const [key, value] of Object.entries(change)) {
			next.searchParams.delete(key);
			if (value !== null && value !== '' && value !== 0) next.searchParams.set(key, String(value));
		}
		navigationError = null;
		try { await goto(`${resolve('/mods')}${next.search}`, { reset: false }); }
		catch { navigationError = '无法更新浏览位置，请重试。'; }
	}
	async function openSource() {
		try {
			const url = new URL(details.value?.project.links.websiteUrl ?? '');
			const hosts = source === 'modrinth' ? ['modrinth.com'] : ['curseforge.com', 'www.curseforge.com'];
			if (url.protocol !== 'https:' || !hosts.includes(url.hostname) || url.username || url.password || url.port) throw new Error('来源链接无效。');
			if (isTauri()) await shell.openExternalUrl(url.href); else window.open(url.href, '_blank', 'noopener,noreferrer');
		} catch { navigationError = '无法打开项目来源链接。'; }
	}
</script>

<div class="flex min-h-0 flex-1 flex-col">
	{#if !projectId}
	<form class="mb-3 flex shrink-0 items-center gap-2" onsubmit={event => { event.preventDefault(); void navigate({ q: draft.trim(), index: 0, project: null }); }}>
		<input type="search" aria-label="搜索模组" placeholder="搜索 Fabric 模组" bind:value={draft} class="h-9 min-w-0 flex-1 rounded-lg bg-chrome-hover px-3 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring" />
		<button type="submit" class={iconButton} aria-label="搜索模组" title="搜索模组"><Search size={16} strokeWidth={1.7} aria-hidden="true" /></button>
		<button type="button" class={iconButton} aria-label="刷新模组列表" title="刷新模组列表" disabled={projects.busy || !configured} onclick={() => void feed.refresh()}><RefreshCw size={15} strokeWidth={1.7} class={projects.busy ? "animate-spin motion-reduce:animate-none" : ""} aria-hidden="true" /></button>
	</form>
	{/if}
	{#if navigationError}<p role="alert" class="mb-3 text-xs text-destructive">{navigationError}</p>{/if}
	{#if configBusy}<p role="status" class="py-8 text-center text-xs text-muted-foreground">正在检查来源配置…</p>
	{:else if configError}<div role="alert" class="rounded-lg border border-border p-4 text-xs"><p>{configError}</p><button type="button" class={`${button} mt-2`} onclick={() => configRevision++}>重新检查来源</button></div>
	{:else if configured}
	<div class="grid min-h-0 flex-1 grid-cols-1 gap-3 min-[1100px]:grid-cols-[minmax(240px,2fr)_minmax(0,3fr)]">
		<div class={`flex min-h-0 flex-col rounded-lg border border-border bg-card ${projectId ? 'hidden min-[1100px]:flex' : ''}`}>
			<div bind:this={listElement} onscroll={() => { if (!projectId && listElement?.getClientRects().length) savedScroll = listElement.scrollTop; }} tabindex="-1" role="region" class="min-h-0 flex-1 space-y-0.5 scrollbar-subtle overflow-y-auto overscroll-contain p-1 focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring" aria-label="模组项目" aria-busy={projects.busy}>
				{#each projects.items as item (item.id)}
					<button type="button" aria-pressed={projectId === String(item.id)} onclick={() => void navigate({ project: String(item.id) })} class={`group flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-left transition-colors focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring ${projectId === String(item.id) ? 'bg-chrome-selected' : 'hover:bg-chrome-hover'}`}>
						<ModProjectIcon project={item} {source} />
						<span class="min-w-0 flex-1"><span class="block truncate text-[13px] font-medium leading-5" title={item.name}>{item.name}</span><span class="mt-0.5 block truncate text-[11px] leading-4 text-muted-foreground" title={item.summary}>{item.summary}</span></span>
						<ModProjectStats project={item} compact />
						<ChevronRight size={13} aria-hidden="true" class={`shrink-0 text-muted-foreground transition-opacity ${projectId === String(item.id) ? 'opacity-80' : 'opacity-30 group-hover:opacity-70 group-focus-visible:opacity-70'}`} />
					</button>
				{:else}{#if !projects.busy && !projects.error && !projects.warning}<p class="px-3 py-10 text-center text-xs text-muted-foreground">没有找到匹配的模组，试试其他关键词。</p>{/if}{/each}
				<div bind:this={sentinel} class="h-px" aria-hidden="true"></div>
				{#if projects.error || projects.warning}<p role="alert" class="px-3 py-2 text-xs text-destructive">{projects.error ?? projects.warning}{projects.items.length ? '（保留已加载结果）' : ''}<button class={button} onclick={() => void feed.retry()} disabled={projects.busy}>重试</button></p>{/if}
				{#if projects.busy}<p role="status" class="flex items-center justify-center gap-2 py-3 text-[11px] text-muted-foreground"><LoaderCircle size={13} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{projects.loaded ? '正在加载更多…' : '正在读取模组…'}</p>
				{:else if projects.nextIndex !== null && !projects.error && !projects.warning}<button type="button" class={`${button} block w-full text-muted-foreground`} onclick={() => void feed.more()}>加载更多</button>
				{:else if projects.items.length && !projects.error && !projects.warning}<p role="status" class="py-3 text-center text-[11px] text-muted-foreground">已显示全部结果</p>{/if}
			</div>
		</div>
		<div class={`flex min-h-0 flex-col overflow-hidden ${!projectId ? 'hidden min-[1100px]:flex' : ''}`}>
			<div class="flex min-h-0 flex-1 flex-col gap-3 overflow-hidden" role="region" aria-label="模组详情内容">
			{#if projectId}
				<section aria-label="模组介绍" class="max-h-[45%] shrink-0 scrollbar-subtle overflow-y-auto overscroll-contain rounded-lg border border-border bg-card p-3.5">
				{#if !validProject}<p role="alert" class="text-xs text-destructive">项目链接无效，请从列表重新选择。</p>{/if}
				{#if details.busy}<p role="status" class="text-xs text-muted-foreground">正在读取项目摘要…</p>{/if}
				{#if details.error || details.value?.refreshError}<p role="alert" class="mb-3 text-xs text-destructive">{details.error ?? details.value?.refreshError}{details.value ? '（保留上次摘要）' : ''}<button type="button" class={button} disabled={details.busy} onclick={() => void loadDetails(true)}>重试</button></p>{/if}
				{#if details.value}
					<div class="flex flex-wrap items-start justify-between gap-3"><div class="flex min-w-0 flex-1 items-center gap-3"><ModProjectIcon project={details.value.project} {source} large /><div class="min-w-0"><h2 class="break-words text-lg font-semibold">{details.value.project.name}</h2>
					{#if details.value.project.authors.length}<p class="mt-1 break-words text-xs text-muted-foreground">{details.value.project.authors.map(author => author.name).join('、')}</p>{/if}</div></div>
					{#if details.value.project.links.websiteUrl}<button class={`${button} ml-auto shrink-0`} onclick={() => void openSource()}>在 {source === 'modrinth' ? 'Modrinth' : 'CurseForge'} 查看项目 ↗</button>{/if}</div>
					<ModProjectStats project={details.value.project} />
					<p class="mt-3 whitespace-pre-wrap break-words text-xs leading-6 text-muted-foreground">{details.value.project.summary}</p>
				{/if}
				</section>
				{#if validProject}
				<section aria-label="兼容版本" class={`flex min-h-0 flex-col rounded-lg border border-border bg-card p-3.5 ${target ? 'flex-1' : 'shrink-0'}`}><div class="flex shrink-0 flex-wrap items-center justify-between gap-x-3 gap-y-1"><h3 class="text-sm font-medium">兼容版本</h3>{#if target}<p class="min-w-0 break-words text-[11px] text-muted-foreground">Minecraft {target.version} · Fabric · {target.name}</p>{/if}</div>
				{#if !target}<p class="mt-2 text-xs leading-5 text-muted-foreground">选择实例以查看兼容版本。</p><button class={`${button} mt-2`} onclick={chooseInstance}>选择实例</button>
				{:else}
					<ModVersionInstaller {source} name={target.name} gameVersion={target.version} project={projectId!} footerTarget={installFooter} />
				{/if}</section>{/if}
			{:else}<p class="py-16 text-center text-xs text-muted-foreground">选择一个模组查看摘要与兼容版本</p>{/if}
			</div>
			<div bind:this={installFooter} class="shrink-0 overflow-hidden rounded-lg border-border empty:hidden [&:not(:empty)]:mt-3 [&:not(:empty)]:border"></div>
		</div>
	</div>{/if}
</div>
