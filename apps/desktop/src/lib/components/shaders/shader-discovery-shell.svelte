<script lang="ts">
	import { onMount, tick } from 'svelte';
	import PageSidebar from '#lib/components/ui/page-sidebar.svelte';
	import { pageSidebarItemClass } from '../ui/page-sidebar-item';
	import { getPageSidebarState } from '../../page-sidebar-state.svelte.js';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { Popover } from 'bits-ui';
	import { ChevronDown, Gamepad2, LoaderCircle, RefreshCw, Search, Sparkles } from '@lucide/svelte';
	import { getClientLibraryState } from '../../client-library-state.svelte.js';
	import { ClientDetailState } from '../../client-detail-state.svelte.js';
	import type { InstalledClient } from '../../runtime';
	import ShaderBrowser from './shader-browser.svelte';
	import SelectableList from '#lib/components/ui/selectable-list.svelte';
	import SelectableRow from '#lib/components/ui/selectable-row.svelte';

	const sidebar = getPageSidebarState();
	const library = getClientLibraryState();
	const detail = new ClientDetailState();
	const targetName = $derived(page.url.searchParams.get('instance'));
	let checkedName = $state<string | null>(null);
	const confirmed = $derived(detail.client && targetName && checkedName === targetName && detail.client.id === `client-${targetName}` && detail.client.name.toLowerCase() === targetName ? detail.client : null);
	const targetIssue = $derived(targetName && checkedName === targetName && !detail.loading && detail.error ? detail.error
		: targetName && checkedName === targetName && detail.client && !confirmed ? '实例信息与所选目标不一致，请重新读取。' : null);
	let pickerOpen = $state(false);
	let instanceSearch = $state('');
	let navigationError = $state<string | null>(null);
	const filtered = $derived(library.clients.filter(c => `${c.name} ${c.versionId}`.toLowerCase().includes(instanceSearch.trim().toLowerCase())));
	const button = 'inline-flex h-8 shrink-0 items-center justify-center gap-1.5 rounded-lg px-2.5 text-xs text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40';

	async function chooseInstance() {
		sidebar.shaders = true;
		await tick();
		pickerOpen = true;
	}
	$effect(() => { if (!sidebar.shaders) pickerOpen = false; });
	onMount(() => { void library.refresh(); });
	$effect(() => {
		checkedName = null;
		if (!targetName) { detail.cancel(); detail.client = null; return; }
		const name = targetName;
		let active = true;
		void detail.load(name).then(() => { if (active) checkedName = name; });
		return () => { active = false; detail.cancel(); };
	});
	async function changeInstance(name: string | null) {
		pickerOpen = false;
		instanceSearch = '';
		navigationError = null;
		try {
			await goto(`${resolve('/shaders')}${name ? `?${new URLSearchParams({ instance: name })}` : ''}`, { reset: false });
		} catch { navigationError = '无法更新光影浏览位置，请重试。'; }
	}
	function refreshInstances() {
		void library.refresh();
		if (targetName) void detail.load(targetName);
	}
</script>

<div class="flex min-h-0 min-w-0 flex-1 overflow-hidden">
	<PageSidebar open={sidebar.shaders} id="shader-discovery-sidebar" label="光影筛选侧栏">
		<h1 class="px-2 pb-4 text-[13px] font-semibold">光影</h1>
		<div class="flex flex-col gap-4">
			<Popover.Root bind:open={pickerOpen}>
				<Popover.Trigger aria-label={targetName ? `选择安装目标，当前请求 ${targetName}` : '选择安装目标，当前未选择实例'} class="inline-flex h-8 min-w-0 items-center gap-2 rounded-lg border border-input px-2.5 text-left text-xs focus-visible:outline-2 focus-visible:outline-ring">
					<Gamepad2 size={14} class="shrink-0 text-muted-foreground" aria-hidden="true" /><span class="min-w-0 flex-1 truncate">{confirmed?.name ?? targetName ?? '选择实例'}</span><ChevronDown size={12} class="shrink-0 text-muted-foreground" aria-hidden="true" />
				</Popover.Trigger>
				{#if pickerOpen}<Popover.Portal to=".app-window">
					<Popover.Content side="bottom" align="start" sideOffset={8} collisionPadding={12} class="z-30 w-80 max-w-[calc(100vw-32px)] rounded-xl border border-border bg-popover p-2 text-popover-foreground shadow-lg outline-none" aria-label="选择光影安装目标">
						<label class="mb-2 flex h-8 items-center gap-2 rounded-lg bg-chrome-hover px-2.5"><Search size={13} class="shrink-0 text-muted-foreground" aria-hidden="true" /><input type="search" bind:value={instanceSearch} placeholder="搜索实例名称或版本" aria-label="搜索目标实例" class="min-w-0 flex-1 bg-transparent text-xs outline-none" /></label>
						{#if library.error}<p role="alert" class="mb-2 px-2 text-[11px] text-destructive">{library.loaded ? '刷新失败，保留上次读取的实例。' : '暂时无法读取实例列表。'}</p>{/if}
						<SelectableList class="max-h-64 overflow-y-auto overscroll-contain" aria-label="目标实例列表" aria-busy={library.loading}>
							<SelectableRow selected={!targetName} onclick={() => void changeInstance(null)} class="h-9"><span class="text-xs">不选择实例</span>{#snippet trailing()}仅浏览{/snippet}</SelectableRow>
							{#each filtered as item (item.id)}
								<SelectableRow selected={targetName === item.name.toLowerCase()} onclick={() => void changeInstance(item.name)} class="h-12" aria-label={`${item.name}，Minecraft ${item.versionId}`}>
									<span class="min-w-0"><span class="block truncate text-xs">{item.name}</span><span class="mt-1 block truncate text-[11px] text-muted-foreground">Minecraft {item.versionId}</span></span>
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
			<div class="flex flex-col gap-1" role="group" aria-label="光影来源">
				<p class="px-2 pb-1 text-[10px] font-medium text-muted-foreground">来源</p>
				<button type="button" aria-pressed="true" class={pageSidebarItemClass(true)}><Sparkles size={15} strokeWidth={1.7} aria-hidden="true" />Modrinth</button>
			</div>
			<p class="px-2 text-[10px] leading-4 text-muted-foreground">首期只支持 Modrinth 和 Iris 兼容版本；CurseForge 光影后续接入。</p>
		</div>
	</PageSidebar>
	<section class="flex min-h-0 min-w-0 flex-1 flex-col" aria-label="光影浏览">
		<div class="flex min-h-0 flex-1 flex-col p-3 lg:p-4">
			<header class="mb-3 shrink-0"><h2 class="text-[13px] font-semibold">光影</h2><p class="mt-1 text-[11px] leading-4 text-muted-foreground">从 Modrinth 浏览 Iris 兼容光影，下载后自动校验并添加到所选实例；游戏内仍需手动选择。</p></header>
			{#if targetIssue}<div role="alert" class="mb-3 flex items-start gap-2 rounded-lg border border-destructive/15 bg-destructive/5 px-3 py-2.5 text-xs"><div class="min-w-0 flex-1"><p class="text-destructive">{targetIssue}</p><div class="mt-2 flex flex-wrap gap-1"><button type="button" class={button} onclick={() => void chooseInstance()}>重新选择实例</button>{#if targetName}<button type="button" class={button} disabled={detail.loading} onclick={() => targetName && void detail.load(targetName)}>重新读取</button>{/if}</div></div></div>{/if}
			{#if navigationError}<p role="alert" class="mb-3 text-xs text-destructive">{navigationError}</p>{/if}
			{#if confirmed}
				{#key confirmed.id}<ShaderBrowser name={confirmed.name} gameVersion={confirmed.versionId} />{/key}
			{:else if targetName && detail.loading}
				<p role="status" class="flex flex-1 items-center justify-center gap-2 text-xs text-muted-foreground"><LoaderCircle size={14} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />正在确认实例…</p>
			{:else}
				<div class="flex flex-1 flex-col items-center justify-center gap-2 text-center">
					<Sparkles size={26} strokeWidth={1.4} class="text-primary/60" aria-hidden="true" />
					<p class="text-[13px]">选择一个实例</p>
					<p class="max-w-80 text-[11px] leading-4 text-muted-foreground">光影版本需要按实例的 Minecraft 版本筛选；选择实例后才能下载并导入。</p>
					<button type="button" onclick={() => void chooseInstance()} class="mt-1 inline-flex h-8 items-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground focus-visible:outline-2 focus-visible:outline-ring">选择实例</button>
				</div>
			{/if}
		</div>
	</section>
</div>
