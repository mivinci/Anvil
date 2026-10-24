<script lang="ts">
	import { onMount, type Snippet } from 'svelte';
	import { ClientInstallState, setClientInstallState } from '../../client-install-state.svelte.js';
	import { ClientLibraryState, setClientLibraryState } from '../../client-library-state.svelte.js';
	import { ClientVersionState, setClientVersionState } from '../../client-version-state.svelte.js';
	import { ResourceInstallState, setResourceInstallState } from '../../resource-install-state.svelte.js';
	import { ClientLaunchState, setClientLaunchState } from '../../client-launch-state.svelte.js';
	import { ClientRemovalState, setClientRemovalState } from '../../client-removal-state.svelte.js';
	const launches = setClientLaunchState(new ClientLaunchState());
	const resources = setResourceInstallState(new ResourceInstallState());
	const versions = setClientVersionState(new ClientVersionState());
	import ClientInstallProgressDialog from './client-install-progress-dialog.svelte';
	let { children }: { children: Snippet } = $props();
	const library = setClientLibraryState(new ClientLibraryState());
	const removal = setClientRemovalState(new ClientRemovalState((name, cleanupPending) => { library.acceptRemoved(name, cleanupPending); launches.forgetClient(name); }));
	const installs = setClientInstallState(new ClientInstallState((receipt) => library.acceptCompleted(receipt)));
	onMount(() => { void launches.sync(); void installs.sync(); void resources.sync(); return () => { removal.dispose(); launches.dispose(); resources.dispose(); installs.dispose(); library.dispose(); versions.dispose(); }; });
</script>
{@render children()}
<ClientInstallProgressDialog />
