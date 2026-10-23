<script lang="ts">
	import { tick } from 'svelte';
	import { page } from '$app/state';
	import ClientCrashReports from '#lib/components/clients/client-crash-reports.svelte';
	import { Copy, RefreshCw } from '@lucide/svelte';
	import { getClientDetailState } from '#lib/client-detail-state.svelte.js';
	import { getClientGameLog, getClientConsoleLog, type ClientConsoleLog, type ClientGameLog } from '#lib/runtime.js';
	const detail = getClientDetailState();
	let viewport = $state<HTMLDivElement>();
	let source = $state<'game' | 'console' | 'crashes'>(page.url.searchParams.get('source') === 'console' ? 'console' : page.url.searchParams.get('source') === 'crashes' ? 'crashes' : 'game');
	let snapshot = $state<ClientGameLog | ClientConsoleLog | null>(null);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let copyStatus = $state<string | null>(null);
	let updated = $state<string | null>(null);
	let request: AbortController | null = null;
	let currentName = '';
	let currentSource = '';
	const isConsole = $derived(source === 'console');
	const readFailed = $derived(snapshot && 'readFailed' in snapshot && snapshot.readFailed);
	async function refresh(name: string, kind: 'game' | 'console' | 'crashes') {
		request?.abort();
		const controller = new AbortController(); request = controller;
		loading = true; error = null; copyStatus = null;
		try {
			const value = await (kind === 'console' ? getClientConsoleLog : getClientGameLog)(name, AbortSignal.any([controller.signal, AbortSignal.timeout(15_000)]));
			if (controller.signal.aborted) return;
			snapshot = value; updated = new Date().toLocaleTimeString();
			await tick();
			if (!controller.signal.aborted && viewport) viewport.scrollTop = viewport.scrollHeight;
		} catch { if (!controller.signal.aborted) error = '无法读取游戏日志，请稍后重试。'; }
		finally { if (request === controller) { request = null; loading = false; } }
	}
	$effect(() => {
		const name = detail.client?.name;
		if (name && source !== 'crashes') {
			if (name !== currentName || source !== currentSource) { currentName = name; currentSource = source; snapshot = null; updated = null; }
			void refresh(name, source);
		}
		return () => { request?.abort(); request = null; };
	});
	async function copy() {
		const text = snapshot?.text;
		if (!text) return;
		try { await navigator.clipboard.writeText(text); copyStatus = '已复制当前日志片段'; }
		catch { copyStatus = '无法复制，请选中文字手动复制。'; }
	}
</script>
<svelte:head><title>{detail.client?.name ?? '实例'} · 游戏日志 · Anvil</title></svelte:head>
<section aria-labelledby="game-log-title" class="flex min-h-0 flex-1 flex-col gap-3 px-5 py-5 lg:px-6">
	<div class="flex items-center justify-between gap-3">
		<div class="flex min-w-0 items-center gap-2"><h1 id="game-log-title" class="text-base font-semibold tracking-tight">游戏日志</h1><span class="text-[11px] text-muted-foreground">{source === 'crashes' ? 'crash-reports' : isConsole ? 'stdout / stderr' : 'latest.log'}</span></div>
		{#if source !== 'crashes'}<div class="flex items-center gap-1">
			<button type="button" disabled={!snapshot?.text} onclick={() => void copy()} aria-label="复制当前日志片段" title="复制当前日志片段" class="grid size-7 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><Copy size={14} aria-hidden="true" /></button>
			<button type="button" disabled={loading || !detail.client} onclick={() => { if (detail.client) void refresh(detail.client.name, source); }} aria-label="刷新游戏日志" title="刷新游戏日志" class="grid size-7 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><RefreshCw size={14} class={loading ? 'animate-spin motion-reduce:animate-none' : ''} aria-hidden="true" /></button>
		</div>{/if}
	</div>
	<div class="flex items-center gap-1 self-start rounded-lg bg-chrome-hover p-1" aria-label="日志来源">
		{#each [{ value: 'game', label: '游戏日志' }, { value: 'console', label: 'Java 输出' }, { value: 'crashes', label: '崩溃报告' }] as option}
			<button type="button" aria-pressed={source === option.value} onclick={() => source = option.value as 'game' | 'console' | 'crashes'} class={`rounded-md px-2.5 py-1 text-[12px] transition-colors focus-visible:outline-2 focus-visible:outline-ring ${source === option.value ? 'bg-chrome-selected font-medium text-accent-foreground' : 'text-muted-foreground hover:text-foreground'}`}>{option.label}</button>
		{/each}
	</div>
	{#if source === 'crashes'}
		<ClientCrashReports />
	{:else}
	{#if readFailed}<p role="status" class="text-[11px] text-muted-foreground">部分 Java 输出未能读取，以下内容可能不完整。</p>{/if}
	{#if error}<p role="alert" class="text-[11px] text-destructive">{error}{snapshot ? ' 保留上次读取的内容。' : ''}</p>{/if}
	<div bind:this={viewport} class="min-h-0 flex-1 overflow-auto overscroll-contain rounded-lg border border-border bg-card">
		{#if snapshot?.text}<pre class="min-w-full w-max select-text p-3 font-mono text-[12px] leading-5 text-foreground"><code>{snapshot.text}</code></pre>
		{:else}<div role="status" class="flex h-full min-h-32 items-center justify-center px-4 text-[13px] text-muted-foreground">{loading ? '正在读取日志…' : snapshot?.exists ? (snapshot.totalBytes ? '当前片段没有可显示的完整行。' : isConsole ? '本次启动暂未产生 Java 输出。' : '日志文件暂时为空。') : snapshot ? (isConsole ? '本次启动器会话中还没有此实例的 Java 输出。' : '还没有游戏日志，启动游戏后可在这里查看。') : '暂未读取到日志。'}</div>{/if}
	</div>
	<div class="flex flex-wrap items-center justify-between gap-2 text-[11px] text-muted-foreground">
		<span>{snapshot?.truncated ? '仅保留末尾内容，部分输出已截断或跳过' : isConsole ? '最近一次启动 · 已过滤认证相关输出' : '游戏自身日志'}{updated ? ` · 更新于 ${updated}` : ''}</span>
		<span role="status">{copyStatus ?? (isConsole ? '最多 256 KiB · 关闭启动器后清空 · 手动刷新' : '点击刷新获取最新内容')}</span>
	</div>
	{/if}
</section>
