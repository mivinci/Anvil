<script lang="ts">
	import { localeState } from '#lib/locale.svelte.ts';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { shellUpgradeRequired } from '../shell-compatibility';
	import { onMount } from 'svelte';
	import { isTauri } from '@tauri-apps/api/core';
	import { ArrowUp, LoaderCircle } from '@lucide/svelte';
	import NavButton from './nav-button.svelte';
	import { shell, type AppUpdateStatus, type ShellUpdateStatus } from '../shell';

	let status = $state<AppUpdateStatus | null>(null);
	let shellUpdate = $state<ShellUpdateStatus | null>(null);
	let runtimeVersion = $state<string | null>(null);
	let restarting = $state(false);
	let restartError = $state(false);
	let refreshTimer: ReturnType<typeof setTimeout> | undefined;

	const shellBlocked = $derived(shellUpgradeRequired(status));
	const shellUpdateAvailable = $derived(shellUpdate?.state === 'available' && shellUpdate.releaseUrl !== null);
	const busy = $derived(status?.state === 'checking' || status?.state === 'downloading' || status?.state === 'verifying' || shellUpdate?.state === 'checking');
	const updateReady = $derived(status?.state === 'staged' || runtimeVersion !== null || shellUpdateAvailable);
	const targetVersion = $derived(shellUpdateAvailable ? shellUpdate?.availableVersion : status?.state === 'staged' ? status.availableVersion : runtimeVersion);
	const buttonTitle = $derived(restartError
		? shellUpdateAvailable ? localeState.messages.common.updatePageFailed : localeState.messages.common.restartFailed
		: shellUpdateAvailable ? localeState.messages.common.shellUpdate(targetVersion)
		: localeState.messages.common.restartUpdate(targetVersion));
	const progress = $derived(status?.totalBytes ? Math.min(100, status.downloadedBytes / status.totalBytes * 100) : 0);

	async function refresh() {
		if (!isTauri()) return;
		try {
			const [updates, runtime] = await Promise.all([shell.getUpdateStatus(), shell.getRuntimeStatus()]);
			status = updates.app;
			shellUpdate = updates.shell;
			runtimeVersion = runtime.availableRuntimeVersion;
		} catch {
			status = null;
			shellUpdate = null;
			runtimeVersion = null;
		}
		refreshTimer = setTimeout(() => void refresh(), busy ? 900 : 60_000);
	}

	async function restartForUpdate() {
		if (!updateReady || restarting) return;
		restarting = true;
		restartError = false;
		try {
			if (shellUpdateAvailable && shellUpdate?.releaseUrl) {
				await shell.openExternalUrl(shellUpdate.releaseUrl);
				restarting = false;
				return;
			}
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

{#if shellBlocked}
	<div class="relative grid size-9 place-items-center">
		<NavButton aria-label={localeState.messages.common.shellUpgradeLabel} title={localeState.messages.common.shellUpgradeTitle} onclick={() => void goto(`${resolve('/settings')}?section=about`)} class="bg-primary/10 text-primary hover:bg-primary/15"><ArrowUp size={17} strokeWidth={2} aria-hidden="true" /></NavButton>
		<span class="pointer-events-none absolute -right-0.5 -top-0.5 size-2 rounded-full bg-primary ring-2 ring-background" aria-hidden="true"></span>
	</div>
{:else if updateReady}
	<div class="relative grid size-9 place-items-center">
		<NavButton
			disabled={restarting}
			aria-label={buttonTitle}
			title={buttonTitle}
			onclick={() => void restartForUpdate()}
			class="bg-primary/10 text-primary hover:bg-primary/15 disabled:opacity-60"
		>
			<ArrowUp size={17} strokeWidth={2} aria-hidden="true" />
		</NavButton>
		<span class="pointer-events-none absolute -right-0.5 -top-0.5 size-2 rounded-full bg-primary ring-2 ring-background" aria-hidden="true"></span>
	</div>
{:else if busy}
	<div class="grid size-9 place-items-center text-primary" role="status" aria-label={localeState.messages.common.updateProgress(status?.totalBytes ? Math.round(progress) : null)} title={localeState.messages.common.updateProgress(status?.totalBytes ? Math.round(progress) : null)}>
		<LoaderCircle size={17} strokeWidth={1.8} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />
	</div>
{/if}
