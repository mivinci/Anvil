<script lang="ts">
	import { onMount, tick } from 'svelte';
	import PageSidebar from '#lib/components/ui/page-sidebar.svelte';
	import { pageSidebarItemClass } from '../ui/page-sidebar-item';
	import { getPageSidebarState } from '../../page-sidebar-state.svelte.js';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { Popover } from 'bits-ui';
	import { Blocks, Flame, ChevronDown, Gamepad2, LoaderCircle, RefreshCw, Search, TriangleAlert } from '@lucide/svelte';
	import { getClientLibraryState } from '../../client-library-state.svelte.js';
	import { ClientDetailState } from '../../client-detail-state.svelte.js';
	import { readModDiscoveryContext, updateModDiscoveryContext } from '../../mod-discovery-context';
	import type { InstalledClient, ModSource } from '../../runtime';
	import ModProjectBrowser from './mod-project-browser.svelte';
	import SelectableList from '#lib/components/ui/selectable-list.svelte';
	import SelectableRow from '#lib/components/ui/selectable-row.svelte';

	const sidebar = getPageSidebarState();
	const library = getClientLibraryState();
	const detail = new ClientDetailState();
	const context = $derived(readModDiscoveryContext(page.url));
	const source = $derived(context.source);
	const targetName = $derived(context.instanceName);
	let checkedName = $state<string | null>(null);
	const client = $derived(detail.client);
	const confirmed = $derived(client && targetName && checkedName === targetName && client.name.toLowerCase() === targetName.toLowerCase() ? client : null);
	const targetIssue = $derived(context.instanceIssue ? '链接中的实例名称无效或重复，请重新选择安装目标。'
		: targetName && checkedName === targetName && !detail.loading && detail.error ? detail.error
		: targetName && checkedName === targetName && client && !confirmed ? '实例信息与所选目标不一致，请重新读取。'
		: confirmed && !detail.loading && !fabric(confirmed) ? '这个实例尚未安装 Fabric，当前不能作为模组安装目标。' : null);
	const ready = $derived(confirmed && !detail.loading && !targetIssue && fabric(confirmed) ? confirmed : null);
	let pickerOpen = $state(false);
	let instanceSearch = $state('');
	let navigating = $state(false);
	let navigationError = $state<string | null>(null);
	const filtered = $derived(library.clients.filter(c => `${c.name} ${c.versionId}`.toLowerCase().includes(instanceSearch.trim().toLowerCase())));
	const button = 'inline-flex h-8 shrink-0 items-center justify-center gap-1.5 rounded-lg px-2.5 text-xs text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40';

	function fabric(value: InstalledClient) {
		return value.loader?.kind === 'fabric' && typeof value.loader.version === 'string' && value.loader.version.length > 0;
	}
	async function chooseInstance() { sidebar.mods = true; await tick(); pickerOpen = true; }
	$effect(() => { if (!sidebar.mods) pickerOpen = false; });
	onMount(() => { void library.refresh(); });
	$effect(() => {
		checkedName = null;
		if (!targetName) { detail.cancel(); detail.client = null; return; }
		const name = targetName;
		let active = true;
		void detail.load(name).then(() => { if (active) checkedName = name; });
		return () => { active = false; detail.cancel(); };
	});
	async function changeContext(change: { instance?: string | null; source?: ModSource }) {
		if (navigating) return;
		pickerOpen = false;
		instanceSearch = '';
		navigationError = null;
		navigating = true;
		try {
			const next = updateModDiscoveryContext(page.url, change);
			if (next.search !== page.url.search) await goto(`${resolve('/mods')}${next.search}`, { reset: false });
		} catch { navigationError = '无法更新模组浏览位置，请重试。'; }
		finally { navigating = false; }
	}
	function refreshInstances() {
		void library.refresh();
		if (targetName) void detail.load(targetName);
	}
</script>

<div class="flex min-h-0 min-w-0 flex-1 overflow-hidden">
	<PageSidebar open={sidebar.mods} id="mod-discovery-sidebar" label="模组筛选侧栏">
		<h1 class="px-2 pb-4 text-[13px] font-semibold">模组</h1>
		<div class="flex flex-col gap-4">
			<div class="flex min-w-0 items-center gap-1.5">
				<Popover.Root bind:open={pickerOpen}>
					<Popover.Trigger disabled={navigating} aria-label={targetName ? `选择安装目标，当前请求 ${targetName}` : '选择安装目标，当前未选择实例'} class="inline-flex h-8 min-w-0 flex-1 items-center gap-2 rounded-lg border border-input px-2.5 text-left text-xs focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40">
						<Gamepad2 size={14} class="shrink-0 text-muted-foreground" aria-hidden="true" /><span class="min-w-0 flex-1 truncate">{confirmed?.name ?? targetName ?? (context.hasInstance ? '无效实例' : '选择实例')}</span><ChevronDown size={12} class="shrink-0 text-muted-foreground" aria-hidden="true" />
					</Popover.Trigger>
					{#if pickerOpen}<Popover.Portal to=".app-window">
					<Popover.Content side="bottom" align="start" sideOffset={8} collisionPadding={12} class="z-30 w-80 max-w-[calc(100vw-32px)] rounded-xl border border-border bg-popover p-2 text-popover-foreground shadow-lg outline-none" aria-label="选择模组安装目标">
						<label class="mb-2 flex h-8 items-center gap-2 rounded-lg bg-chrome-hover px-2.5"><Search size={13} class="shrink-0 text-muted-foreground" aria-hidden="true" /><input type="search" bind:value={instanceSearch} placeholder="搜索实例名称或版本" aria-label="搜索目标实例" class="min-w-0 flex-1 bg-transparent text-xs outline-none" /></label>
						{#if library.error}<p role="alert" class="mb-2 px-2 text-[11px] text-destructive">{library.loaded ? '刷新失败，保留上次读取的实例。' : '暂时无法读取实例列表。'}</p>{/if}
						<SelectableList class="max-h-64 overflow-y-auto overscroll-contain" aria-label="目标实例列表" aria-busy={library.loading}>
							<SelectableRow selected={!context.hasInstance} onclick={() => void changeContext({ instance: null })} class="h-9"><span class="text-xs">不选择实例</span>{#snippet trailing()}仅浏览{/snippet}</SelectableRow>
							{#each filtered as item (item.id)}
								<SelectableRow selected={targetName === item.name.toLowerCase()} disabled={!fabric(item) || navigating} onclick={() => void changeContext({ instance: item.name })} class="h-12" aria-label={`${item.name}，Minecraft ${item.versionId}，${fabric(item) ? 'Fabric' : '暂不支持安装模组'}`}>
									<span class="min-w-0"><span class="block truncate text-xs">{item.name}</span><span class="mt-1 block truncate text-[11px] text-muted-foreground">Minecraft {item.versionId}{fabric(item) ? ` · Fabric ${item.loader?.version}` : ' · 未安装 Fabric'}</span></span>
								</SelectableRow>
							{/each}
						</SelectableList>
						{#if !library.loaded && library.loading}<p role="status" class="flex items-center justify-center gap-2 py-4 text-xs text-muted-foreground"><LoaderCircle size={13} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />读取实例中…</p>
						{:else if library.loaded && !library.clients.length}<p class="px-2 py-4 text-xs text-muted-foreground">还没有实例，可以先前往客户端安装游戏。</p>
						{:else if library.loaded && !filtered.length}<p class="px-2 py-4 text-xs text-muted-foreground">没有匹配的实例。</p>{/if}
						<div class="mt-2 flex justify-between border-t border-border pt-2"><a href={resolve('/clients')} class={button}>管理客户端</a><button type="button" class={button} disabled={library.loading} onclick={refreshInstances}><RefreshCw size={12} class={library.loading ? 'animate-spin motion-reduce:animate-none' : ''} aria-hidden="true" />刷新实例</button></div>
					</Popover.Content>
					</Popover.Portal>{/if}
				</Popover.Root>
			</div>
			<div class="flex flex-col gap-1" role="group" aria-label="模组来源">
				{#each [{ id: 'modrinth', name: 'Modrinth', icon: Blocks }, { id: 'curseforge', name: 'CurseForge', icon: Flame }] as source}
					<button type="button" aria-pressed={context.source === source.id} disabled={navigating} onclick={() => void changeContext({ source: source.id as ModSource })} class={pageSidebarItemClass(context.source === source.id)}><source.icon size={15} strokeWidth={1.7} aria-hidden="true" />{source.name}</button>
				{/each}
			</div>
		</div>
	</PageSidebar>
	<section class="flex min-h-0 min-w-0 flex-1 flex-col" aria-label="模组浏览">
		<div class="flex min-h-0 flex-1 flex-col p-3 lg:p-4">
			{#if context.sourceIssue}<div role="alert" class="mb-3 rounded-lg border border-destructive/15 bg-destructive/5 px-3 py-2.5 text-xs text-destructive">链接中的模组来源无效或重复，请重新选择 Modrinth 或 CurseForge。</div>{/if}
			{#if targetIssue}<div role="alert" class="mb-3 flex items-start gap-2 rounded-lg border border-destructive/15 bg-destructive/5 px-3 py-2.5 text-xs"><TriangleAlert size={14} class="mt-0.5 shrink-0 text-destructive" aria-hidden="true" /><div class="min-w-0 flex-1"><p class="text-destructive">{targetIssue}</p><div class="mt-2 flex flex-wrap gap-1"><button type="button" class={button} disabled={navigating} onclick={() => void chooseInstance()}>重新选择实例</button>{#if targetName}<button type="button" class={button} disabled={detail.loading} onclick={() => targetName && void detail.load(targetName)}>重新读取</button>{/if}<button type="button" class={button} disabled={navigating} onclick={() => void changeContext({ instance: null })}>仅浏览模组</button></div></div></div>{/if}
			{#if navigationError}<p role="alert" class="mb-3 text-xs text-destructive">{navigationError}</p>{/if}
			{#if source && (!context.hasInstance || ready)}
				<ModProjectBrowser {source} target={ready ? { name: targetName!, version: ready.versionId } : null} chooseInstance={() => void chooseInstance()} />
			{:else if !targetIssue && context.hasInstance}
				<p role="status" class="py-10 text-center text-xs text-muted-foreground">确认目标实例后显示兼容模组…</p>
			{/if}
		</div>
	</section>
</div>
