<script lang="ts">
	import { onMount, type Snippet } from 'svelte';
	import { ClientInstallState, setClientInstallState } from '../../client-install-state.svelte.js';
	import { ClientLibraryState, setClientLibraryState } from '../../client-library-state.svelte.js';
	import { ClientVersionState, setClientVersionState } from '../../client-version-state.svelte.js';
	import { ResourceInstallState, setResourceInstallState } from '../../resource-install-state.svelte.js';
	import { ClientLaunchState, setClientLaunchState } from '../../client-launch-state.svelte.js';
	import { ClientRemovalState, setClientRemovalState } from '../../client-removal-state.svelte.js';
	import { ModpackExportState, setModpackExportState } from '../../modpack-export-state.svelte.js';
	const launches = setClientLaunchState(new ClientLaunchState());
	const resources = setResourceInstallState(new ResourceInstallState());
	const exports = setModpackExportState(new ModpackExportState());
	const versions = setClientVersionState(new ClientVersionState());
	import ClientInstallProgressDialog from './client-install-progress-dialog.svelte';
	import ModpackExportToast from './modpack-export-toast.svelte';
	let { children }: { children: Snippet } = $props();
	const library = setClientLibraryState(new ClientLibraryState());
	const removal = setClientRemovalState(new ClientRemovalState((name, cleanupPending) => { library.acceptRemoved(name, cleanupPending); launches.forgetClient(name); }));
	const installs = setClientInstallState(new ClientInstallState((receipt) => library.acceptCompleted(receipt)));
	onMount(() => { void launches.sync(); void installs.sync(); void resources.sync(); void exports.sync(); return () => { removal.dispose(); launches.dispose(); resources.dispose(); exports.dispose(); installs.dispose(); library.dispose(); versions.dispose(); }; });
</script>
{@render children()}
<ClientInstallProgressDialog />
<ModpackExportToast />

{#if installs.starting && installs.importing || installs.operation?.importRequestId && installs.active && installs.operation.progress.stage === 'parsing'}
	<div role="status" class="fixed bottom-5 left-1/2 z-50 -translate-x-1/2 rounded-xl bg-card px-4 py-3 text-xs text-foreground shadow-lg">{installs.starting ? '正在读取并解析整合包…' : '正在解析整合包…'}<span class="ml-2 text-muted-foreground">可在下载任务中查看进度</span></div>
{/if}
