<script lang="ts">
	import { getClientDetailState } from '#lib/client-detail-state.svelte.js';
	import { resolve } from '$app/paths';
	import { getResourceInstallState } from '#lib/resource-install-state.svelte.js';
	const resources = getResourceInstallState();
	let refreshedOperation = $state<number | null>(null);
	const detail = getClientDetailState();
	import { page } from '$app/state';
	import { onDestroy } from 'svelte';
	import { Popover } from 'bits-ui';
	import { Boxes, Check, Copy, FileArchive, Hash, LoaderCircle, Pause, Play, Plus, RefreshCw, Trash2 } from '@lucide/svelte';
	import { listClientResources, importClientMod, changeClientMod, syncClientResources, RuntimeRequestError, type ClientResourceManifest, type ClientResourceEntry } from '../../../../lib/runtime';
	let manifest = $state<ClientResourceManifest | null>(null);
	let busy = $state(false);
	let error = $state<string | null>(null);
	let uncertain = $state(false);
	let selected = $state<File | null>(null);
	let removing = $state<string | null>(null);
	let search = $state('');
	let copyTarget = $state<string | null>(null);
	let copyOk = $state(false);
	let copyTimer: ReturnType<typeof setTimeout> | undefined;
	let input: HTMLInputElement;
	let request: AbortController | null = null;
	const fabric = $derived(detail.client?.loader?.kind === 'fabric');
	const applied = $derived(fabric && manifest !== null && manifest.appliedRevision === manifest.revision);
	const name = $derived(page.params.name ?? '');
	const canBrowse = $derived(fabric && !busy && !uncertain);
	const discoveryHref = $derived(`${resolve('/mods')}?${new URLSearchParams({ instance: name.toLowerCase() })}`);
	const visible = $derived(manifest?.mods.filter((mod) => mod.filename.toLowerCase().includes(search.trim().toLowerCase())) ?? []);
	type ModRelation = { requires: string[]; requiredBy: { name: string; enabled: boolean }[] };
	const relations = $derived.by(() => {
		const map = new Map<string, ModRelation>();
		const mods = manifest?.mods ?? [];
		const idOf = (entry: ClientResourceEntry) => entry.modrinthOrigin?.projectId ?? entry.origin?.projectId;
		const dependenciesOf = (entry: ClientResourceEntry): (string | number)[] => entry.modrinthOrigin ? Object.keys(entry.modrinthOrigin.requiredVersions) : entry.origin ? entry.origin.requiredProjects : [];
		const nameOf = (id: string | number) => {
			const entry = mods.find((candidate) => String(idOf(candidate)) === String(id));
			return entry ? entry.modrinthOrigin?.projectName ?? entry.origin?.projectName ?? entry.filename : `项目 ${id}`;
		};
		for (const mod of mods) {
			const own = idOf(mod);
			const requiredBy = own === undefined ? [] : mods
				.filter((other) => other.sha256 !== mod.sha256 && dependenciesOf(other).some((id) => String(id) === String(own)))
				.map((other) => ({ name: other.modrinthOrigin?.projectName ?? other.origin?.projectName ?? other.filename, enabled: other.enabled }));
			map.set(mod.sha256, { requires: dependenciesOf(mod).map(nameOf), requiredBy });
		}
		return map;
	});
	function validate(result: ClientResourceManifest) {
		// Schema 5 adds shaderPacks; this page only reads the mods collection.
		if (!result || ![1, 2, 3, 4, 5].includes(result.schemaVersion) || !Number.isSafeInteger(result.revision) || !Array.isArray(result.mods) || result.mods.length > 1024 || result.mods.some((entry) => typeof entry.filename !== 'string' || !/^[a-f0-9]{64}$/.test(entry.sha256) || !Number.isSafeInteger(entry.size) || typeof entry.enabled !== 'boolean' || !['local', 'curseforge', 'modrinth'].includes(entry.source))) throw new Error('资源清单格式不正确。');
		return result;
	}
	async function run(action: (signal: AbortSignal) => Promise<ClientResourceManifest>, mutation = false) {
		if (request) return;
		const controller = new AbortController(); request = controller;
		busy = true; error = null;
		try {
			const result = validate(await action(AbortSignal.any([controller.signal, AbortSignal.timeout(60_000)])));
			if (!controller.signal.aborted) { manifest = result; uncertain = false; if (mutation) { selected = null; removing = null; } }
		} catch (reason) {
			if (!controller.signal.aborted) {
				const rejected = reason instanceof RuntimeRequestError && reason.status >= 400 && reason.status < 500;
				if (mutation && !rejected) uncertain = true;
				error = rejected
					? reason instanceof Error ? reason.message : '无法应用此变更。'
					: mutation ? `操作未确认，请刷新清单后再操作。${reason instanceof Error ? reason.message : ''}` : '暂时无法读取资源清单，请刷新重试。';
			}
		} finally { if (request === controller) { request = null; busy = false; } }
	}
	function refresh() { const target = name; void run((signal) => listClientResources(target, signal)); }
	$effect(() => {
		const target = name;
		manifest = null; selected = null; removing = null; search = ''; uncertain = false;
		void run((signal) => listClientResources(target, signal));
		return () => { request?.abort(); request = null; busy = false; };
	});
	$effect(() => {
		const operation = resources.operation;
		if (operation && operation.state !== 'running' && operation.name === name.toLowerCase() && refreshedOperation !== operation.id && !busy) {
			refreshedOperation = operation.id;
			refresh();
		}
	});
	function pick(event: Event) {
		const element = event.currentTarget as HTMLInputElement;
		const file = element.files?.[0]; element.value = '';
		if (!file) return;
		if (!file.name.toLowerCase().endsWith('.jar') || file.size === 0 || file.size > 64 * 1024 * 1024) { error = '请选择不超过 64 MiB 的非空 .jar 文件。'; return; }
		selected = file; error = null;
	}
	function importFile() {
		if (!manifest || !selected || busy || uncertain) return;
		const target = name, file = selected, revision = manifest.revision;
		void run((signal) => importClientMod(target, file, revision, signal), true);
	}
	function change(mod: ClientResourceEntry, enabled: boolean | null) {
		if (!manifest || busy || uncertain) return;
		const target = name, revision = manifest.revision;
		void run((signal) => changeClientMod(target, mod.sha256, revision, enabled, signal), true);
	}
	function apply() {
		if (!manifest || busy || uncertain || !fabric) return;
		const target = name, revision = manifest.revision;
		void run((signal) => syncClientResources(target, revision, signal), true);
	}
	function size(bytes: number) { return bytes < 1024 * 1024 ? `${(bytes / 1024).toFixed(1)} KiB` : `${(bytes / 1024 / 1024).toFixed(1)} MiB`; }
	async function copyHash(hash: string) {
		if (copyTimer) clearTimeout(copyTimer);
		try { await navigator.clipboard.writeText(hash); copyTarget = hash; copyOk = true; }
		catch { copyTarget = hash; copyOk = false; }
		copyTimer = setTimeout(() => { copyTarget = null; copyTimer = undefined; }, 1500);
	}
	onDestroy(() => { if (copyTimer) clearTimeout(copyTimer); });
</script>

<svelte:head><title>模组 · Anvil</title></svelte:head>
<section aria-labelledby="client-mods-title" class="flex min-h-0 flex-1 flex-col overflow-y-auto overscroll-contain px-5 py-5 lg:px-6">
	<div class="flex items-center justify-between gap-3"><div class="flex items-baseline gap-2"><h1 id="client-mods-title" class="text-base font-semibold tracking-tight">模组</h1>{#if manifest}<span class="text-[11px] text-muted-foreground">{manifest.mods.length} 个引用</span>{/if}</div><div class="flex gap-1.5">{#if fabric && manifest && applied}<button type="button" disabled={busy || uncertain} onclick={apply} title="检查并同步受管模组文件" class="h-7 rounded-md px-2 text-xs text-muted-foreground hover:bg-chrome-hover disabled:opacity-40">同步文件</button>{/if}<button type="button" disabled={busy} aria-label="刷新模组清单" title="刷新" onclick={refresh} class="grid size-7 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><RefreshCw size={14} class={busy ? 'animate-spin motion-reduce:animate-none' : ''} aria-hidden="true" /></button><button type="button" disabled={busy || !manifest || uncertain} onclick={() => input.click()} class="inline-flex h-7 items-center gap-1.5 rounded-lg px-2 text-xs text-muted-foreground hover:bg-chrome-hover disabled:opacity-40">本地导入</button><a href={canBrowse ? discoveryHref : undefined} aria-disabled={!canBrowse} tabindex={canBrowse ? 0 : -1} aria-label={`为 ${name} 添加模组`} title={fabric ? "浏览兼容模组" : "需要 Fabric 实例"} class="inline-flex h-7 items-center gap-1.5 rounded-lg bg-primary px-2.5 text-xs font-medium text-primary-foreground focus-visible:outline-2 focus-visible:outline-ring aria-disabled:pointer-events-none aria-disabled:opacity-40"><Plus size={13} aria-hidden="true" />添加模组</a></div></div>
	<input bind:this={input} type="file" accept=".jar" aria-label="选择本地模组 JAR" onchange={pick} class="hidden" />
	<p class="mt-2 text-[11px] leading-5 text-muted-foreground">{fabric ? '启用的文件会写入本实例的 mods 目录；共享原件保留，启动前会重新检查文件。平台资源保留必需依赖关系；本地导入文件尚不解析依赖与兼容性。' : '此实例为原版，可导入并管理引用；启用模组需要 Fabric 实例。'}</p>
	{#if error}<p role="alert" class="mt-3 wrap-break-word rounded-lg bg-destructive/5 px-3 py-2 text-xs text-destructive">{error}</p>{/if}
	{#if fabric && manifest && !applied && manifest.mods.some((mod) => mod.enabled)}<div class="mt-3 flex items-center justify-between gap-3 rounded-lg border border-border px-3 py-2"><span class="text-[11px] text-muted-foreground">有启用的引用尚未应用到游戏目录。</span><button type="button" disabled={busy || uncertain} onclick={apply} class="shrink-0 rounded-md px-2 py-1 text-xs text-primary hover:bg-chrome-hover disabled:opacity-40">应用到游戏</button></div>{/if}
	{#if selected}
		<div class="mt-3 rounded-lg border border-border bg-card p-3"><div class="flex items-center gap-2"><FileArchive size={15} class="shrink-0 text-muted-foreground" aria-hidden="true" /><span class="min-w-0 flex-1 truncate text-[13px]" title={selected.name}>{selected.name}</span><span class="text-[11px] text-muted-foreground">{size(selected.size)}</span></div><p class="mt-2 text-[11px] leading-4 text-muted-foreground">相同内容复用共享原件；新引用默认停用。仅检查 JAR 格式，尚不解析兼容性与依赖。</p><div class="mt-2 flex justify-end gap-2"><button type="button" disabled={busy} onclick={() => selected = null} class="h-7 rounded-md px-2 text-xs text-muted-foreground hover:bg-chrome-hover">取消</button><button type="button" disabled={busy || uncertain} onclick={importFile} class="h-7 rounded-md bg-primary px-2.5 text-xs text-primary-foreground disabled:opacity-40">{busy ? '导入中…' : '确认导入'}</button></div></div>
	{/if}
	{#if !manifest && busy}<div role="status" class="flex min-h-40 items-center justify-center gap-2 text-xs text-muted-foreground"><LoaderCircle size={15} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />读取资源清单…</div>
	{:else if manifest}
		{#if manifest.mods.length === 0}<div class="flex min-h-48 flex-1 flex-col items-center justify-center gap-3 text-muted-foreground"><Boxes size={25} strokeWidth={1.5} aria-hidden="true" /><p class="text-xs">尚未导入模组</p></div>
		{:else}
			<input type="search" bind:value={search} placeholder="搜索模组文件名" aria-label="搜索模组" class="mt-4 h-8 w-full rounded-lg bg-chrome-hover px-2.5 text-xs outline-none focus:ring-1 focus:ring-ring" />
			<ul class="mt-3 divide-y divide-border rounded-lg border border-border bg-card px-3" aria-label="本实例模组引用">
				{#each visible as mod (mod.sha256)}
					{@const relation = relations.get(mod.sha256) ?? { requires: [], requiredBy: [] }}
					<li class="py-2.5"><div class="flex items-center gap-2"><div class="min-w-0 flex-1"><p class="truncate text-[13px]" title={mod.filename}>{mod.modrinthOrigin?.projectName ?? mod.origin?.projectName ?? mod.filename}</p><p class="mt-1 text-[11px] text-muted-foreground">{mod.source === 'modrinth' ? `Modrinth · ${mod.modrinthOrigin?.direct ? '直接添加' : '必需依赖'}` : mod.source === 'curseforge' ? `CurseForge · ${mod.origin?.direct ? '直接添加' : '必需依赖'}` : '本地导入'} · {size(mod.size)} · {mod.enabled ? applied ? '已启用' : '待应用' : '已停用'}</p></div><button type="button" disabled={busy || uncertain || (!fabric && !mod.enabled)} aria-pressed={mod.enabled} aria-label={`${mod.enabled ? '停用' : '启用'} ${mod.filename}`} title={mod.enabled ? '停用' : '启用'} onclick={() => change(mod, !mod.enabled)} class="grid size-7 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40">{#if mod.enabled}<Pause size={13} aria-hidden="true" />{:else}<Play size={13} aria-hidden="true" />{/if}</button><Popover.Root><Popover.Trigger aria-label="查看文件身份" title="文件身份" class="grid size-7 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring"><Hash size={13} aria-hidden="true" /></Popover.Trigger><Popover.Portal to=".app-window"><Popover.Content side="bottom" align="end" sideOffset={6} collisionPadding={8} class="z-30 w-80 max-w-[calc(100vw-32px)] rounded-xl border border-border bg-popover p-3 text-popover-foreground shadow-lg outline-none" aria-label="文件身份"><p class="truncate text-xs font-medium" title={mod.filename}>{mod.filename}</p><p class="mt-1 text-[11px] text-muted-foreground">{mod.source === 'modrinth' ? 'Modrinth' : mod.source === 'curseforge' ? 'CurseForge' : '本地导入'} · {size(mod.size)}</p><p class="mt-2 text-[10px] text-muted-foreground">SHA-256</p><p class="mt-1 select-all break-all font-mono text-[10px] leading-4">{mod.sha256}</p>{#if relation.requires.length || relation.requiredBy.length}<div class="mt-2 space-y-1 border-t border-border pt-2 text-[11px] leading-5">{#if relation.requires.length}<p><span class="text-muted-foreground">依赖：</span>{relation.requires.join('、')}</p>{/if}{#if relation.requiredBy.length}<p><span class="text-muted-foreground">被依赖：</span>{relation.requiredBy.map((item) => `${item.name}${item.enabled ? '' : '（已停用）'}`).join('、')}</p>{/if}</div>{/if}<div class="mt-2 flex justify-end"><button type="button" onclick={() => void copyHash(mod.sha256)} class="inline-flex h-7 items-center gap-1.5 rounded-md px-2 text-xs text-primary hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring">{#if copyTarget === mod.sha256 && copyOk}<Check size={12} aria-hidden="true" />{:else}<Copy size={12} aria-hidden="true" />{/if}{copyTarget === mod.sha256 ? (copyOk ? '已复制' : '复制失败') : '复制 SHA-256'}</button></div></Popover.Content></Popover.Portal></Popover.Root><button type="button" disabled={busy || uncertain} aria-label={`移除 ${mod.filename}`} title="移除本实例引用" onclick={() => removing = mod.sha256} class="grid size-7 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover hover:text-destructive focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><Trash2 size={13} aria-hidden="true" /></button></div>
						{#if removing === mod.sha256}<div class="mt-2 flex flex-wrap items-center justify-end gap-2 rounded-md bg-chrome-hover px-2 py-2 text-[11px]"><span class="mr-auto text-muted-foreground">只移除此实例引用，共享原件保留。</span><button type="button" disabled={busy} onclick={() => removing = null} class="rounded px-1.5 py-1 text-muted-foreground">取消</button><button type="button" disabled={busy || uncertain} onclick={() => change(mod, null)} class="rounded px-1.5 py-1 text-destructive disabled:opacity-40">确认移除</button></div>{/if}
					</li>
				{/each}
			</ul>
			{#if visible.length === 0}<p class="py-5 text-center text-xs text-muted-foreground">没有匹配的模组</p>{/if}
		{/if}
	{/if}
</section>
