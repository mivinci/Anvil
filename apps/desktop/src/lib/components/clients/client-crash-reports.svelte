<script lang="ts">
	import { untrack } from 'svelte';
	import { Copy, RefreshCw } from '@lucide/svelte';
	import { getClientDetailState } from '../../client-detail-state.svelte.js';
	import { listClientCrashReports, getClientCrashReport, RuntimeRequestError, type ClientCrashReports, type ClientCrashReport, type ClientCrashReportEntry } from '../../runtime';
	const detail = getClientDetailState();
	let catalog = $state<ClientCrashReports | null>(null);
	let report = $state<ClientCrashReport | null>(null);
	let selected = $state<string | null>(null);
	let listing = $state(false);
	let reading = $state(false);
	let listError = $state<string | null>(null);
	let readError = $state<string | null>(null);
	let copied = $state<string | null>(null);
	let listRequest: AbortController | null = null;
	let readRequest: AbortController | null = null;
	let currentName = '';
	const describeError = (error: unknown) => error instanceof RuntimeRequestError ? error.message : '无法读取报告，请稍后重试。';
	const size = (bytes: number) => bytes < 1024 ? `${bytes} B` : bytes < 1024 * 1024 ? `${(bytes / 1024).toFixed(0)} KiB` : `${(bytes / 1024 / 1024).toFixed(1)} MiB`;
	async function select(name: string, entry: ClientCrashReportEntry) {
		readRequest?.abort(); readRequest = null;
		selected = entry.name; report = null; readError = null; copied = null; reading = false;
		if (entry.size > (catalog?.maxReportBytes ?? 2 * 1024 * 1024)) { readError = '报告超过 2 MiB，暂不支持在应用内查看。'; return; }
		const controller = new AbortController(); readRequest = controller; reading = true;
		try {
			const value = await getClientCrashReport(name, entry.name, AbortSignal.any([controller.signal, AbortSignal.timeout(15_000)]));
			if (!controller.signal.aborted) report = value;
		} catch (error) { if (!controller.signal.aborted) readError = describeError(error); }
		finally { if (readRequest === controller) { readRequest = null; reading = false; } }
	}
	async function refresh(name: string) {
		listRequest?.abort(); readRequest?.abort(); readRequest = null; reading = false;
		const controller = new AbortController(); listRequest = controller; listing = true; listError = null;
		try {
			const value = await listClientCrashReports(name, AbortSignal.any([controller.signal, AbortSignal.timeout(15_000)]));
			if (controller.signal.aborted) return;
			catalog = value;
			const entry = value.reports.find(item => item.name === selected) ?? value.reports[0];
			if (entry) void select(name, entry);
			else { selected = null; report = null; readError = null; copied = null; }
		} catch (error) { if (!controller.signal.aborted) listError = describeError(error); }
		finally { if (listRequest === controller) { listRequest = null; listing = false; } }
	}
	$effect(() => {
		const name = detail.client?.name;
		if (name) untrack(() => {
			if (name !== currentName) { currentName = name; catalog = null; report = null; selected = null; copied = null; readError = null; }
			void refresh(name);
		});
		return () => { listRequest?.abort(); readRequest?.abort(); listRequest = null; readRequest = null; };
	});
	async function copy() {
		const value = report;
		if (!value) return;
		try { await navigator.clipboard.writeText(value.text); if (report === value) copied = '已复制报告全文'; }
		catch { if (report === value) copied = '无法复制，请选中文字手动复制。'; }
	}
</script>
<div class="flex min-h-0 flex-1 flex-col gap-2">
	<div class="flex min-h-7 items-center justify-between gap-2 text-[11px] text-muted-foreground">
		<span>{catalog ? `${catalog.reports.length} 份报告${catalog.truncated ? ' · 仅显示最近 50 份' : ''}` : 'game/crash-reports'}</span>
		<div class="flex items-center gap-1">
			<button type="button" disabled={!report?.text} onclick={() => void copy()} aria-label="复制崩溃报告全文" title="复制报告全文" class="grid size-7 place-items-center rounded-md hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><Copy size={14} aria-hidden="true" /></button>
			<button type="button" disabled={listing || !detail.client} onclick={() => { if (detail.client) void refresh(detail.client.name); }} aria-label="刷新崩溃报告" title="刷新崩溃报告" class="grid size-7 place-items-center rounded-md hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><RefreshCw size={14} class={listing ? 'animate-spin motion-reduce:animate-none' : ''} aria-hidden="true" /></button>
		</div>
	</div>
	{#if listError}<p role="alert" class="text-[11px] text-destructive">{listError}{catalog ? ' 保留上次读取的列表。' : ''}</p>{/if}
	<div class="flex min-h-0 flex-1 overflow-hidden rounded-lg border border-border bg-card">
		{#if catalog?.reports.length}
			<nav aria-label="崩溃报告列表" class="w-40 shrink-0 space-y-1 overflow-y-auto overscroll-contain border-r border-border p-1.5 lg:w-48">
				{#each catalog.reports as entry (entry.name)}
					<button type="button" disabled={listing} aria-pressed={selected === entry.name} onclick={() => { if (detail.client) void select(detail.client.name, entry); }} title={entry.name} class={`block w-full rounded-md px-2 py-2 text-left focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-50 ${selected === entry.name ? 'bg-chrome-selected text-accent-foreground' : 'hover:bg-chrome-hover'}`}>
						<span class="block truncate text-[12px] leading-4">{entry.name}</span>
						<span class="mt-1 block truncate text-[11px] leading-4 text-muted-foreground">{entry.modifiedAt !== null ? new Date(entry.modifiedAt).toLocaleString(undefined, { year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' }) : '时间未知'} · {size(entry.size)}</span>
					</button>
				{/each}
			</nav>
			<div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
				<p class="shrink-0 truncate border-b border-border px-3 py-2 text-[11px] text-muted-foreground" title={selected ?? undefined}>{selected}</p>
				<div class="min-h-0 flex-1 overflow-auto overscroll-contain">
					{#if report?.text}<pre class="w-max min-w-full select-text p-3 font-mono text-[12px] leading-5"><code>{report.text}</code></pre>
					{:else}<div role={readError ? 'alert' : 'status'} class={`flex h-full min-h-32 items-center justify-center px-4 text-[13px] ${readError ? 'text-destructive' : 'text-muted-foreground'}`}><span>{reading ? '正在读取报告…' : readError ?? (report ? '报告文件为空。' : '请选择一份报告。')}{#if readError && detail.client}<button type="button" class="ml-2 text-primary" onclick={() => { const entry = catalog?.reports.find(item => item.name === selected); if (entry && detail.client) void select(detail.client.name, entry); }}>重试</button>{/if}</span></div>{/if}
				</div>
			</div>
		{:else}<div role="status" class="flex min-h-32 flex-1 items-center justify-center px-4 text-[13px] text-muted-foreground">{listing ? '正在读取崩溃报告…' : catalog ? '暂无崩溃报告' : '暂未读取到报告列表。'}</div>{/if}
	</div>
	<p role="status" class="text-[11px] text-muted-foreground">{copied ?? '最新报告排在前面 · 单份最多 2 MiB · 手动刷新'}</p>
</div>
