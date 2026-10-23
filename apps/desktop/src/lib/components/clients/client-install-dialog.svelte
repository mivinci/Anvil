<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { getClientInstallState } from '../../client-install-state.svelte.js';
	const installs = getClientInstallState();
	import { Dialog } from 'bits-ui';
	import ClientLoaderPicker, { type ClientLoader } from './client-loader-picker.svelte';
	let loader = $state<ClientLoader>('vanilla');
	let loaderVersion = $state<string | null>(null);
	const loaderReady = $derived(loader === 'vanilla' || (loader === 'fabric' && loaderVersion !== null));
	const loaderName = $derived(({ vanilla: '原版', fabric: 'Fabric', neoforge: 'NeoForge', forge: 'Forge' })[loader]);
	import SelectableRow from '../ui/selectable-row.svelte';
	import SelectableList from '../ui/selectable-list.svelte';
	import { LoaderCircle, RefreshCw, Search, X } from '@lucide/svelte';
	import { type GameVersion, type InstalledClient } from '../../runtime';
	let { clients, onClose }: { clients: InstalledClient[]; onClose: () => void } = $props();
	import { getClientVersionState } from '../../client-version-state.svelte.js';
	const catalog = getClientVersionState();
	const versions = $derived(catalog.versions);
	const loading = $derived(catalog.loading);
	const error = $derived(catalog.error);
	let channel = $state<'release' | 'snapshot'>('release');
	let query = $state('');
	let selected = $state<GameVersion | null>(null);
	let name = $state('');
	let nameEdited = $state(false);
	let touched = $state(false);
	const visible = $derived(versions.filter((version) => version.type === channel && version.id.toLowerCase().includes(query.trim().toLowerCase())));
	const nameError = $derived(!name ? '请输入实例名称。' : !/^[a-zA-Z0-9][a-zA-Z0-9._-]{0,47}$/.test(name) || name.endsWith('.') ? '名称需以英文字母或数字开头，仅可使用英文、数字、点、下划线和短横线，最多 48 位，不能以点结尾。' : clients.some((client) => client.name.toLowerCase() === name.toLowerCase()) ? '已有同名实例，请换一个名称（不区分大小写）。' : null);
	function select(version: GameVersion) {
		selected = version;
	}
	$effect(() => {
		if (selected && versions.some((version) => version.id === selected?.id)) return;
		const first = versions.find((version) => version.type === channel);
		if (first) select(first);
		else selected = null;
	});
	$effect(() => {
		if (nameEdited || !selected) return;
		const suffix = loader === 'vanilla' ? '' : `-${loader}`;
		name = selected.id.replace(/[^a-zA-Z0-9._-]/g, '-').slice(0, 48 - suffix.length).replace(/\.+$/, '') + suffix;
	});
	async function submit() {
		touched = true;
		if (!selected || nameError || !installs.canStart || !catalog.loaded || !loaderReady) return;
		void installs.start(name, selected.id, loader === 'fabric' && loaderVersion ? { kind: 'fabric', version: loaderVersion } : null);
		onClose();
		await tick();
		installs.detailsOpen = true;
	}
	onMount(() => { void catalog.load(); });
</script>

<Dialog.Root open onOpenChange={(open) => { if (!open) onClose(); }}>
	<Dialog.Overlay class="fixed inset-0 z-40 bg-black/15 backdrop-blur-[2px]" />
	<Dialog.Content class="fixed top-1/2 left-1/2 z-50 flex max-h-[calc(100dvh-48px)] w-135 max-w-[calc(100vw-48px)] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-2xl border border-border bg-popover text-popover-foreground shadow-[0_12px_48px_rgb(0_0_0/0.16)] outline-none" onInteractOutside={(event) => event.preventDefault()} onCloseAutoFocus={(event) => { event.preventDefault(); document.querySelector<HTMLButtonElement>('[data-client-install-trigger]')?.focus(); }}>
		<header class="shrink-0 px-5 pt-5 pb-4"><Dialog.Title class="text-base font-semibold">安装 Minecraft</Dialog.Title><Dialog.Description class="mt-1 text-[11px] text-muted-foreground">选择游戏版本和加载器，创建独立的客户端实例。</Dialog.Description><button type="button" aria-label="关闭安装面板" onclick={onClose} class="absolute top-4 right-4 grid size-6 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring"><X size={14} aria-hidden="true" /></button></header>
		<div class="min-h-0 overflow-y-auto overscroll-contain px-5 pb-4">
			<div class="flex items-center justify-between gap-3"><div role="group" aria-label="版本类型" class="inline-flex rounded-[10px] bg-chrome-hover p-0.75">{#each [{ value: 'release', label: '正式版' }, { value: 'snapshot', label: '快照' }] as option}<button type="button" aria-pressed={channel === option.value} onclick={() => { channel = option.value as 'release' | 'snapshot'; }} class={`rounded-[7px] px-3 py-1 text-xs focus-visible:outline-2 focus-visible:outline-ring ${channel === option.value ? 'bg-chrome-selected text-accent-foreground' : 'text-muted-foreground'}`}>{option.label}</button>{/each}</div><div class="flex items-center gap-1"><button type="button" aria-label="刷新版本目录" title="刷新版本目录" disabled={loading} onclick={() => void catalog.load(true)} class="grid size-8 shrink-0 place-items-center rounded-lg text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><RefreshCw size={13} class={loading ? 'animate-spin motion-reduce:animate-none' : ''} aria-hidden="true" /></button><label class="flex h-8 w-44 items-center gap-2 rounded-lg bg-chrome-hover px-2.5 text-muted-foreground focus-within:ring-1 focus-within:ring-ring"><Search size={13} aria-hidden="true" /><input type="search" aria-label="搜索 Minecraft 版本" placeholder="搜索版本号" bind:value={query} class="min-w-0 flex-1 bg-transparent text-xs text-foreground outline-none placeholder:text-muted-foreground" /></label></div></div>
			<div class="mt-3 h-52 overflow-y-auto overscroll-contain rounded-lg border border-border" aria-busy={loading}>
				{#if !catalog.loaded && loading}<div role="status" class="flex h-full items-center justify-center gap-2 text-xs text-muted-foreground"><LoaderCircle size={15} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />正在获取版本目录…</div>
				{:else if !catalog.loaded && error}<div class="p-4 text-xs" role="alert"><p class="text-destructive">无法获取游戏版本。</p><button type="button" onclick={() => void catalog.load(true)} class="mt-2 rounded px-2 py-1 text-primary hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring">重新获取</button><details class="mt-2 text-[11px] text-muted-foreground"><summary>错误详情</summary><p class="mt-1 select-text wrap-break-word">{error}</p></details></div>
				{:else if visible.length === 0}<p class="flex h-full items-center justify-center text-xs text-muted-foreground">{query ? '没有匹配的版本' : '暂无此类型的版本'}</p>
				{:else}
					<SelectableList role="group" aria-label="可安装版本">
						{#each visible as version (version.id)}
							<SelectableRow selected={selected?.id === version.id} aria-pressed={selected?.id === version.id} onclick={() => select(version)}>
								<span class="truncate">{version.id}</span>
								{#snippet trailing()}{version.releaseTime.slice(0, 10)}{/snippet}
							</SelectableRow>
						{/each}
					</SelectableList>
				{/if}
			</div>
			{#if catalog.loaded && error}<p role="alert" class="mt-2 text-[11px] leading-4 text-destructive">更新失败，继续显示已有版本。<button type="button" disabled={loading} onclick={() => void catalog.load(true)} class="ml-1 underline underline-offset-2">重试</button></p>{/if}
			<ClientLoaderPicker bind:value={loader} bind:version={loaderVersion} gameVersion={selected?.id ?? null} />
			<div class="mt-4"><label for="client-instance-name" class="text-[13px] font-medium">实例名称</label><input id="client-instance-name" value={name} oninput={(event) => { name = event.currentTarget.value; nameEdited = true; touched = true; }} onblur={() => touched = true} maxlength={48} autocomplete="off" spellcheck="false" aria-invalid={touched && nameError !== null} aria-describedby="client-name-hint" placeholder="例如 1.21.1-survival" class="mt-2 h-8 w-full rounded-lg border border-input bg-main-pane px-2.5 text-[13px] outline-none focus:ring-1 focus:ring-ring" /><p id="client-name-hint" class={`mt-1.5 text-[11px] leading-4 ${touched && nameError ? 'text-destructive' : 'text-muted-foreground'}`}>{touched && nameError ? nameError : '英文、数字及 . _ -，最多 48 位；安装位置由 Anvil 管理。'}</p></div>
			{#if !installs.canStart}<div class="mt-3 text-[11px] text-muted-foreground">{installs.active ? '已有安装任务尚未结束。' : installs.starting ? '正在创建安装任务。' : '正在确认任务状态，或需要重新查询。'}<button type="button" onclick={() => { onClose(); installs.detailsOpen = true; }} class="ml-1 rounded px-1 py-0.5 text-primary hover:bg-chrome-hover">查看任务</button></div>{/if}
		</div>
		<footer class="flex shrink-0 items-center justify-between gap-3 border-t border-border px-5 py-3"><span class="min-w-0 truncate text-[11px] text-muted-foreground">{selected ? `${selected.id} · ${loaderName}${loaderVersion ? ` ${loaderVersion}` : ''}` : '尚未选择版本'}</span><div class="flex shrink-0 gap-2"><button type="button" onclick={onClose} class="h-8 rounded-lg px-3 text-xs text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring">关闭</button><button type="button" onclick={submit} disabled={!selected || !!nameError || !installs.canStart || !catalog.loaded || !loaderReady} class="h-8 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground disabled:opacity-40">{loader === 'vanilla' || loader === 'fabric' ? '开始安装' : '暂未开放安装'}</button></div></footer>
	</Dialog.Content>
</Dialog.Root>
