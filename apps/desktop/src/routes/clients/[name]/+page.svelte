<script lang="ts">
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { untrack } from 'svelte';
	import { listClientResources } from '#lib/runtime.js';
	import { validateClientResourcePacks } from '#lib/client-resource-pack-state.svelte.js';
	import { validateShaderPackChange } from '#lib/shader-packs.js';
	import ClientScreenPreview from '#lib/components/clients/client-screen-preview.svelte';
	import ClientLaunchControls from '#lib/components/clients/client-launch-controls.svelte';
	import ClientModpackExport from '#lib/components/clients/client-modpack-export.svelte';
	import { getClientLaunchState, launchActive, launchLabel } from '#lib/client-launch-state.svelte.js';
	import { Boxes, Image, Sparkles, ArrowUpRight, ChevronDown, Check, Copy } from '@lucide/svelte';
	import { getClientDetailState } from '#lib/client-detail-state.svelte.js';
	const detail = getClientDetailState();
	const name = $derived(page.params.name ?? '');
	let packCount = $state<number | null>(null);
	let shaderCount = $state<number | null>(null);
	let modCount = $state<number | null>(null);
	let countLoading = $state(false);
	let countError = $state(false);
	let countRequest: AbortController | null = null;
	async function loadCount(target: string) {
		countRequest?.abort();
		const controller = new AbortController(); countRequest = controller;
		countLoading = true; countError = false;
		try {
			const manifest = await listClientResources(target, AbortSignal.any([controller.signal, AbortSignal.timeout(20_000)]));
			if (![1, 2, 3, 4, 5].includes(manifest.schemaVersion)) throw new Error('资源清单版本无效');
			const common = { revision: manifest.revision, appliedRevision: manifest.appliedRevision ?? null };
			const textures = validateClientResourcePacks({ ...common, packs: manifest.resourcePacks ?? [] });
			const shaders = validateShaderPackChange({ ...common, packs: manifest.shaderPacks ?? [] });
			if (countRequest === controller && !controller.signal.aborted) { modCount = manifest.mods.length; packCount = textures.packs.length; shaderCount = shaders.packs.length; }
		} catch {
			if (countRequest === controller && !controller.signal.aborted) { modCount = null; packCount = null; shaderCount = null; countError = true; }
		} finally {
			if (countRequest === controller) { countRequest = null; countLoading = false; }
		}
	}
	$effect(() => {
		const target = name;
		untrack(() => { modCount = null; packCount = null; shaderCount = null; void loadCount(target); });
		return () => { countRequest?.abort(); countRequest = null; };
	});
	const launches = getClientLaunchState();
	const operation = $derived(detail.client ? launches.forClient(detail.client.name) : undefined);
	const status = $derived(operation && launchActive(operation) ? launchLabel(operation) : '未运行');
	const links = $derived(detail.client ? [
		{ title: '模组', hint: modCount !== null ? `${modCount} 个模组` : countLoading ? '读取数量…' : countError ? '数量暂不可用' : '管理与添加模组', icon: Boxes, href: resolve('/clients/[name]/mods', { name: detail.client.name.toLowerCase() }) },
		{ title: '资源包', hint: packCount !== null ? `${packCount} 个资源包` : countLoading ? '读取数量…' : countError ? '数量暂不可用' : '查看与管理资源包', icon: Image, href: resolve('/clients/[name]/resource-packs', { name: detail.client.name.toLowerCase() }) },
		{ title: '光影', hint: shaderCount !== null ? `${shaderCount} 个光影` : countLoading ? '读取数量…' : countError ? '数量暂不可用' : '查看与管理光影', icon: Sparkles, href: resolve('/clients/[name]/shader-packs', { name: detail.client.name.toLowerCase() }) },
	] : []);
	let copied = $state<string | null>(null);
	let copyError = $state<string | null>(null);
	let copying = $state(false);
	const fields = $derived(detail.client ? [
		{ label: '版本类型', value: detail.client.versionType === 'release' ? '正式版' : detail.client.versionType === 'snapshot' ? '快照' : detail.client.versionType, copy: false },
		{ label: 'Java 要求', value: `Java ${detail.client.javaMajor}`, copy: false },
		{ label: '安装目标', value: `${detail.client.os} · ${detail.client.architecture}`, copy: false },
		{ label: '记录文件', value: `${detail.client.fileCount} 个`, copy: false },
	] : []);
	async function copy(label: string, value: string) {
		if (copying) return;
		copying = true; copied = null; copyError = null;
		try { await navigator.clipboard.writeText(value); copied = label; }
		catch { copyError = '无法复制，请选中文字手动复制。'; }
		finally { copying = false; }
	}
</script>

<svelte:head><title>{detail.client?.name ?? '实例'} · 概览 · Anvil</title></svelte:head>
<section aria-labelledby="client-overview-title" class="min-h-0 flex-1 overflow-y-auto overscroll-contain px-5 py-5 lg:px-6">
	{#if detail.client}
		<div class="relative isolate flex items-center overflow-hidden rounded-xl bg-primary/5 p-4">
			<div class="relative z-10 min-w-0 flex-1">
				<p class="mb-1 flex items-center gap-1.5 text-[11px] text-muted-foreground"><span class={`size-1.5 rounded-full ${operation && launchActive(operation) ? 'bg-primary' : 'bg-muted-foreground/50'}`}></span>{status}</p>
				<h1 id="client-overview-title" class="max-w-full break-all text-lg font-semibold tracking-tight">{detail.client.name}</h1>
				<div class="mt-1.5 flex flex-wrap gap-1.5 text-[11px] text-muted-foreground"><span class="rounded-md bg-card/70 px-2 py-1">Minecraft {detail.client.versionId}</span><span class="rounded-md bg-card/70 px-2 py-1">{detail.client.loader ? `Fabric ${detail.client.loader.version}` : '原版'}</span></div>
				<div class="mt-3"><ClientLaunchControls client={detail.client}>{#snippet trailing()}<ClientModpackExport client={detail.client!} />{/snippet}</ClientLaunchControls></div>
			</div>
			<div class="ml-4 w-[38%] max-w-80 shrink-0"><ClientScreenPreview /></div>
		</div>
		<div class="mt-5 grid grid-cols-3 gap-2">
			{#each links as item (item.title)}
				<a href={item.href} title={(item.title === '模组' || item.title === '资源包' || item.title === '光影') && countError ? '数量暂时无法读取，点击进入管理；可刷新概览重试。' : undefined} class="group flex min-w-0 items-start gap-2.5 rounded-xl bg-card p-2.5 transition-colors hover:bg-card-hover focus-visible:outline-2 focus-visible:outline-ring">
					<span class="grid size-8 shrink-0 place-items-center rounded-lg bg-primary/8 text-primary"><item.icon size={17} strokeWidth={1.5} aria-hidden="true" /></span>
					<div class="min-w-0 flex-1">
						<div class="flex items-start justify-between gap-2">
							<h2 class="truncate text-[13px] font-medium">{item.title}</h2>
							<ArrowUpRight size={13} class="mt-0.5 shrink-0 text-muted-foreground/50 transition-transform motion-safe:group-hover:-translate-y-0.5 motion-safe:group-hover:translate-x-0.5" aria-hidden="true" />
						</div>
						<p class="mt-1 truncate text-[11px] leading-4 text-muted-foreground" title={item.hint}>{item.hint}</p>
					</div>
				</a>
			{/each}
		</div>
		<details class="group mt-5 border-t border-border pt-3">
			<summary class="flex cursor-pointer list-none items-center justify-between rounded-md py-1 text-xs text-muted-foreground focus-visible:outline-2 focus-visible:outline-ring [&::-webkit-details-marker]:hidden">安装信息<ChevronDown size={14} class="transition-transform group-open:rotate-180 motion-reduce:transition-none" aria-hidden="true" /></summary>
			<dl class="mt-2 grid grid-cols-2 gap-x-6 gap-y-3 rounded-lg bg-card p-3">
				{#each fields as field (field.label)}<div class="min-w-0"><dt class="text-[11px] text-muted-foreground">{field.label}</dt><dd class="mt-1 select-text break-all text-xs">{field.value}</dd></div>{/each}
			</dl>
			<div class="mt-2 flex gap-3">{#each [{ label: '实例名称', value: detail.client.name }, { label: '游戏版本', value: detail.client.versionId }] as item}<button type="button" disabled={copying} onclick={() => void copy(item.label, item.value)} class="inline-flex items-center gap-1 rounded px-1 py-1 text-[11px] text-muted-foreground hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring">{#if copied === item.label}<Check size={12} aria-hidden="true" />{:else}<Copy size={12} aria-hidden="true" />{/if}复制{item.label}</button>{/each}</div>
		</details>
	{/if}
	{#if copied}<p role="status" class="mt-2 text-[11px] text-muted-foreground">已复制{copied}</p>{/if}
	{#if copyError}<p role="alert" class="mt-2 text-[11px] text-destructive">{copyError}</p>{/if}
</section>
