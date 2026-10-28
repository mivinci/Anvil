<script lang="ts">
	import { onDestroy, tick } from 'svelte';
	import { Check, Download } from '@lucide/svelte';
	import { Popover } from 'bits-ui';
	import { getClientLaunchState, launchActive, launchLabel } from '../client-launch-state.svelte.js';
	import ClientJavaProgress from './clients/client-java-progress.svelte';
	import { getClientInstallState } from '../client-install-state.svelte.js';
	import ClientInstallProgress from './clients/client-install-progress.svelte';
	import { getResourceInstallState } from '../resource-install-state.svelte.js';
	import ResourceInstallProgress from './clients/resource-install-progress.svelte';
	import ModpackExportProgress from './clients/modpack-export-progress.svelte';
	import { getModpackExportState } from '../modpack-export-state.svelte.js';

	const launches = getClientLaunchState();
	const activeJavaTasks = $derived(launches.javaTasks.filter(value => launchActive(value) && !['running', 'stopping'].includes(value.phase)));
	const activeJava = $derived(activeJavaTasks[0]);
	const resources = getResourceInstallState();
	const exports = getModpackExportState();
	const installs = getClientInstallState();
	let open = $state(false);

	const finishedClient = $derived(!!installs.operation && !installs.active && !installs.starting);
	const finishedResource = $derived(!!resources.operation && !resources.active && !resources.starting);
	const finishedExport = $derived(!!exports.operation && !exports.active && !exports.starting);
	const javaBudget = $derived(Math.max(0, 3 - Number(finishedClient) - Number(finishedResource) - Number(finishedExport)));
	const visibleJavaTasks = $derived.by(() => {
		const all = launches.javaTasks;
		const recentTerminal = all.filter(task => !launchActive(task)).sort((a, b) => b.id - a.id).slice(0, javaBudget);
		const keep = new Set(recentTerminal.map(task => task.id));
		return all.filter(task => launchActive(task) || keep.has(task.id));
	});

	const RING_RADIUS = 10.5;
	const RING_CIRCUMFERENCE = 2 * Math.PI * RING_RADIUS;

	let completed = $state(false);
	let completedTimer: ReturnType<typeof setTimeout> | undefined;
	let seenActive = false;
	let tracked = new Set<string>();

	const hasActive = $derived(installs.active || installs.starting || resources.active || resources.starting || exports.active || exports.starting || activeJavaTasks.length > 0);

	const aggregate = $derived.by(() => {
		let downloaded = 0;
		let total = 0;
		let count = 0;
		let indeterminate = installs.starting || resources.starting || exports.starting;
		let allPaused = true;
		const add = (taskDownloaded: number, taskTotal: number | null, downloading: boolean, paused: boolean) => {
			count += 1;
			if (!paused) allPaused = false;
			if (!downloading || taskTotal === null || taskTotal <= 0) indeterminate = true;
			else { downloaded += taskDownloaded; total += taskTotal; }
		};
		if (installs.active) {
			const operation = installs.operation;
			if (operation) add(operation.progress.downloadedBytes, operation.progress.totalBytes, operation.progress.stage === 'downloading', operation.state === 'paused');
			else indeterminate = true;
		}
		if (resources.active) {
			const operation = resources.operation;
			if (operation) add(operation.progress.downloadedBytes, operation.progress.totalBytes, operation.progress.stage === 'downloading', false);
			else indeterminate = true;
		}
		if (exports.active || exports.starting) indeterminate = true;
		for (const task of activeJavaTasks) add(task.javaDownloadedBytes, task.javaTotalBytes, task.javaStage === 'downloading', false);
		if (count === 0) { allPaused = false; indeterminate = true; }
		return { downloaded, total, indeterminate, allPaused: count > 0 && allPaused };
	});

	const determinate = $derived(hasActive && !aggregate.indeterminate && aggregate.total > 0);
	const percent = $derived(determinate ? Math.min(100, aggregate.downloaded / aggregate.total * 100) : 0);
	const spinning = $derived(hasActive && !determinate && !aggregate.allPaused);
	const arcFraction = $derived(determinate ? percent / 100 : 0.25);
	const ringOffset = $derived(RING_CIRCUMFERENCE * (1 - arcFraction));

	const currentLabel = $derived(
		exports.active || exports.starting ? exports.label
			: resources.active || resources.starting ? resources.label
			: installs.active || installs.starting ? installs.label
				: activeJava ? launchLabel(activeJava)
					: null,
	);
	const triggerLabel = $derived(completed
		? '任务：已完成'
		: currentLabel ? `任务：${currentLabel}${determinate ? ` ${Math.round(percent)}%` : ''}` : '任务');

	const activeIds = $derived.by(() => {
		const ids = new Set<string>();
		if (installs.active && installs.operation) ids.add(`install:${installs.operation.id}`);
		if (resources.active && resources.operation) ids.add(`resource:${resources.operation.id}`);
		if (exports.active && exports.operation) ids.add(`export:${exports.operation.id}`);
		for (const task of activeJavaTasks) ids.add(`java:${task.id}`);
		return ids;
	});

	function finished(id: string) {
		const [kind, raw] = id.split(':');
		const value = Number(raw);
		if (kind === 'install') return !!installs.operation && installs.operation.id === value && installs.operation.state === 'completed';
		if (kind === 'resource') return !!resources.operation && resources.operation.id === value && resources.operation.state === 'completed';
		if (kind === 'export') return !!exports.operation && exports.operation.id === value && exports.operation.state === 'completed';
		const task = launches.javaTasks.find(item => item.id === value);
		return !!task && (task.javaStage === 'completed' || task.phase === 'running');
	}

	$effect(() => {
		const active = hasActive;
		const ids = activeIds;
		if (active) {
			seenActive = true;
			tracked = ids;
			completed = false;
			if (completedTimer) { clearTimeout(completedTimer); completedTimer = undefined; }
			return;
		}
		if (!seenActive) return;
		seenActive = false;
		const anyFinished = [...tracked].some(finished);
		tracked = new Set();
		if (!anyFinished) return;
		completed = true;
		completedTimer = setTimeout(() => { completed = false; completedTimer = undefined; }, 2000);
	});

	onDestroy(() => { if (completedTimer) clearTimeout(completedTimer); });
</script>

<Popover.Root bind:open>
	<Popover.Trigger data-download-tasks-trigger class="relative grid size-6 cursor-default place-items-center rounded-md text-chrome-muted/80 transition-colors hover:bg-chrome-selected hover:text-chrome-muted focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring data-[state=open]:bg-chrome-selected" aria-label={triggerLabel} title={triggerLabel}>
		{#if completed}
			<svg class="pointer-events-none absolute inset-0 size-6 text-primary" viewBox="0 0 24 24" fill="none" aria-hidden="true">
				<circle cx="12" cy="12" r={RING_RADIUS} stroke="currentColor" stroke-width="1.5" />
			</svg>
			<Check size={14} strokeWidth={2.2} class="download-complete relative text-primary" aria-hidden="true" />
		{:else}
			<Download size={hasActive ? 13 : 16} strokeWidth={hasActive ? 1.8 : 2} aria-hidden="true" />
			{#if hasActive}
				<svg class={`pointer-events-none absolute inset-0 size-6 text-primary ${spinning ? 'download-ring-spin' : ''}`} viewBox="0 0 24 24" fill="none" aria-hidden="true">
					<circle cx="12" cy="12" r={RING_RADIUS} stroke="currentColor" stroke-opacity=".16" stroke-width="1.5" />
					<circle class="download-ring-progress" cx="12" cy="12" r={RING_RADIUS} stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-dasharray={RING_CIRCUMFERENCE} stroke-dashoffset={ringOffset} transform="rotate(-90 12 12)" />
				</svg>
			{/if}
		{/if}
	</Popover.Trigger>
	<Popover.Content side="bottom" align="end" sideOffset={8} collisionPadding={8} class="z-30 max-h-[calc(100dvh-80px)] w-72 overflow-y-auto overscroll-contain cursor-default rounded-xl border border-chrome-control-border bg-popover p-3 text-popover-foreground shadow-lg outline-none" aria-label="任务">
		<h2 class="mb-3 text-sm font-semibold">任务</h2>
		{#if installs.operation || installs.starting || installs.error}<ClientInstallProgress onNavigate={() => open = false} /><button type="button" onclick={async () => { open = false; await tick(); installs.detailsOpen = true; }} class="mt-1.5 w-full rounded-md py-1.5 text-xs text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring">查看详情</button>
		{:else if !resources.operation && !resources.error && !resources.starting && !exports.active && !exports.starting && !exports.error && !exports.uncertain && launches.javaTasks.length === 0}<div class="flex flex-col items-center gap-3 py-6 text-chrome-muted"><Download size={24} strokeWidth={1.5} aria-hidden="true" /><p class="text-xs">{installs.syncing ? '正在读取任务…' : '暂无进行中的任务'}</p></div>{/if}
		{#if resources.operation || resources.error || resources.starting}<div class={installs.operation || installs.error ? 'mt-2.5 border-t border-border pt-2.5' : ''}><ResourceInstallProgress /></div>{/if}
		{#if exports.active || exports.starting || exports.error || exports.uncertain || exports.operation?.state === 'failed' || exports.operation?.state === 'cancelled'}<div class={installs.operation || installs.error || resources.operation || resources.starting || resources.error ? 'mt-2.5 border-t border-border pt-2.5' : ''}><ModpackExportProgress /></div>{/if}
		{#each visibleJavaTasks as operation, index (operation.id)}
			<div class={index > 0 || installs.operation || installs.starting || installs.error || resources.operation || resources.starting || resources.error ? 'mt-2.5 border-t border-border pt-2.5' : ''}><ClientJavaProgress {operation} /></div>
		{/each}
	</Popover.Content>
</Popover.Root>

<style>
	.download-ring-progress { transition: stroke-dashoffset .3s ease; }
	.download-ring-spin { animation: download-ring-spin 1.7s linear infinite; transform-origin: center; }
	.download-complete { animation: download-complete .18s ease-out; transform-origin: center; }
	@keyframes download-ring-spin { to { transform: rotate(360deg); } }
	@keyframes download-complete { from { opacity: 0; transform: scale(.8); } }
	@media (prefers-reduced-motion: reduce) {
		.download-ring-progress { transition: none; }
		.download-ring-spin, .download-complete { animation: none; }
	}
</style>
