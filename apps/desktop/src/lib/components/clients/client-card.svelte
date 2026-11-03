<script lang="ts">
	import { localeState } from '#lib/locale.svelte.ts';
	import { resolve } from '$app/paths';
	import ClientAvatar from './client-avatar.svelte';
	import ClientLaunchControls from './client-launch-controls.svelte';
	import type { InstalledClient } from '../../runtime';
	import { getClientLaunchState, launchActive, launchLabel, launchNeedsLogin } from '../../client-launch-state.svelte.js';
	let { client }: { client: InstalledClient } = $props();
	const launches = getClientLaunchState();
	const operation = $derived(launches.forClient(client.name));
	const active = $derived(launchActive(operation));
	const label = $derived(operation ? launchLabel(operation) : localeState.messages.clients.flow.launch);
	const downloading = $derived(active && operation?.javaStage === 'downloading');
	const progress = $derived(downloading && operation?.javaTotalBytes ? Math.min(100, operation.javaDownloadedBytes / operation.javaTotalBytes * 100) : null);
	const statusBadge = $derived(
		operation?.phase === 'running' ? (operation.offlineTest ? localeState.messages.clients.flow.offlineTest : localeState.messages.clients.flow.running)
			: launchNeedsLogin(operation) ? localeState.messages.clients.flow.needsLogin
			: operation?.phase === 'failed' ? localeState.messages.clients.flow.launchFailed
				: operation?.phase === 'cancelled' ? localeState.messages.clients.flow.cancelled
					: operation?.phase === 'stopping' ? localeState.messages.clients.flow.stopping
						: null,
	);
	const badgeTone = $derived(operation?.phase === 'failed' ? 'text-destructive' : operation?.phase === 'cancelled' ? 'text-muted-foreground' : 'text-muted-foreground');
</script>
<li class="min-w-0">
	<div class="group relative isolate overflow-hidden rounded-xl bg-card p-3 transition-[transform,border-color,box-shadow] duration-200 hover:border-primary/30 hover:shadow-sm motion-safe:hover:-translate-y-0.5">
		<a href={resolve('/clients/[name]', { name: client.name.toLowerCase() })} aria-label={localeState.messages.clients.manageInstance(client.name)} class="absolute inset-0 z-10 rounded-xl focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring"></a>
		<div class="relative flex min-w-0 items-center gap-2.5">
			<ClientAvatar id={client.id} name={client.name} class="size-12" />
			<div class="min-w-0 flex-1">
				<h2 class="truncate text-body font-medium" title={client.name}>{client.name}</h2>
				<p class="mt-1 truncate text-caption text-muted-foreground" title={client.loader ? `Minecraft ${client.versionId} · Fabric ${client.loader.version}` : `Minecraft ${client.versionId}`}>{client.versionId}{client.loader ? ` · Fabric ${client.loader.version}` : localeState.messages.clients.flow.vanillaSuffix}</p>
				{#if statusBadge}<p class={`mt-0.5 truncate text-xs ${badgeTone}`} title={label}>{statusBadge}</p>{/if}
			</div>
			<div class="relative z-20"><ClientLaunchControls {client} compact /></div>
		</div>
		{#if downloading}<progress aria-label={localeState.messages.clients.flow.javaDownload} max="100" value={progress ?? undefined} class="absolute inset-x-0 bottom-0 h-0.5 w-full accent-primary [&::-webkit-progress-bar]:bg-chrome-hover [&::-webkit-progress-value]:bg-primary"></progress>{/if}
	</div>
</li>
