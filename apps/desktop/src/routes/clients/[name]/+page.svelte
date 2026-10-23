<script lang="ts">
	import { Check, Copy, RefreshCw } from '@lucide/svelte';
	import { getClientDetailState } from '#lib/client-detail-state.svelte.js';
	const detail = getClientDetailState();
	let copied = $state<string | null>(null);
	let copyError = $state<string | null>(null);
	let copying = $state(false);
	const fields = $derived(detail.client ? [
		{ label: '实例名称', value: detail.client.name, copy: true },
		{ label: '游戏版本', value: detail.client.versionId, copy: true },
		{ label: '游戏类型', value: detail.client.loader ? `Fabric ${detail.client.loader.version}` : 'Minecraft Java · 原版', copy: false },
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
	<div class="flex items-center justify-between gap-3"><h1 id="client-overview-title" class="text-base font-semibold tracking-tight">概览</h1><button type="button" aria-label="刷新实例详情" title="刷新实例详情" disabled={detail.loading || !detail.client} onclick={() => { if (detail.client) void detail.load(detail.client.name); }} class="grid size-7 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><RefreshCw size={14} class={detail.loading ? 'animate-spin motion-reduce:animate-none' : ''} aria-hidden="true" /></button></div>
	<dl class="mt-4 divide-y divide-border rounded-lg border border-border bg-card px-3.5">
		{#each fields as field (field.label)}
			<div class="flex min-h-10 items-center justify-between gap-4 py-2"><dt class="shrink-0 text-[13px]">{field.label}</dt><dd class="flex min-w-0 items-center gap-1"><span class="min-w-0 select-text break-all text-right text-xs text-muted-foreground">{field.value}</span>{#if field.copy}<button type="button" aria-label={`复制${field.label}`} title={`复制${field.label}`} disabled={copying} onclick={() => void copy(field.label, field.value)} class="grid size-6 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40">{#if copied === field.label}<Check size={12} class="text-primary" aria-hidden="true" />{:else}<Copy size={12} aria-hidden="true" />{/if}</button>{/if}</dd></div>
		{/each}
	</dl>
	<p class="mt-3 text-[11px] leading-5 text-muted-foreground">安装记录已读取，尚未检查游戏文件完整性。Java 要求来自安装记录，不代表已安装的 Java 版本。</p>
	{#if copied}<p role="status" class="mt-2 text-[11px] text-muted-foreground">已复制{copied}</p>{/if}
	{#if copyError}<p role="alert" class="mt-2 text-[11px] text-destructive">{copyError}</p>{/if}
</section>
