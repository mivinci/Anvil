<script lang="ts">
	import { onMount } from 'svelte';
	import { isTauri } from '@tauri-apps/api/core';
	import { Download } from '@lucide/svelte';
	import { shell, type AppUpdateStatus } from '../shell';

	let status = $state<AppUpdateStatus | null>(null);
	let runtimeVersion = $state<string | null>(null);
	let restarting = $state(false);
	let restartError = $state(false);
	let refreshTimer: ReturnType<typeof setTimeout> | undefined;

	const busy = $derived(status?.state === 'checking' || status?.state === 'downloading' || status?.state === 'verifying');
	const updateReady = $derived(status?.state === 'staged' || runtimeVersion !== null);
	const targetVersion = $derived(status?.state === 'staged' ? status.availableVersion : runtimeVersion);
	const progress = $derived(status?.totalBytes ? Math.min(100, status.downloadedBytes / status.totalBytes * 100) : 0);

	async function refresh() {
		if (!isTauri()) return;
		try {
			const [updates, runtime] = await Promise.all([shell.getUpdateStatus(), shell.getRuntimeStatus()]);
			status = updates.app;
			runtimeVersion = runtime.availableRuntimeVersion;
		} catch {
			status = null;
			runtimeVersion = null;
		}
		refreshTimer = setTimeout(() => void refresh(), busy ? 900 : 60_000);
	}

	async function restartForUpdate() {
		if (!updateReady || restarting) return;
		restarting = true;
		restartError = false;
		try {
			await shell.restartForUpdate();
		} catch {
			restartError = true;
			restarting = false;
		}
	}

	onMount(() => {
		void refresh();
		return () => {
			if (refreshTimer) clearTimeout(refreshTimer);
		};
	});
</script>

{#if updateReady}
	<button
		type="button"
		disabled={restarting}
		aria-label={restartError ? '更新已准备好，重启失败，请重试' : `重启以更新到 ${targetVersion ?? '新版本'}`}
		title={restartError ? '重启失败，请重试' : `重启以更新到 ${targetVersion ?? '新版本'}`}
		onclick={() => void restartForUpdate()}
		class="relative flex w-10 flex-col items-center justify-center gap-0.5 rounded-lg border border-primary/25 bg-primary/10 px-1 py-1.5 text-primary shadow-sm transition-colors hover:bg-primary/15 focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-60"
	>
		<Download size={15} strokeWidth={1.9} aria-hidden="true" />
		<span class="text-[9px] leading-3 font-semibold">{restarting ? '重启中' : '更新'}</span>
		<span class="absolute -right-0.5 -top-0.5 size-2 rounded-full bg-primary ring-2 ring-background" aria-hidden="true"></span>
	</button>
{:else if busy}
	<div class="grid size-10 place-items-center rounded-lg text-primary" role="status" aria-label="正在准备应用更新" title="正在准备应用更新">
		<div class="h-5 w-5 overflow-hidden rounded-md border border-primary/20 bg-primary/5">
			<div class="h-full bg-primary/25 transition-[width]" style={`width:${progress}%`}></div>
		</div>
	</div>
{/if}
