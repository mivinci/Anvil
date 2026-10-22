<script lang="ts">
	import { onMount, type Snippet } from 'svelte';
	import { ClientInstallState, setClientInstallState } from '../../client-install-state.svelte.js';
	import { ClientLibraryState, setClientLibraryState } from '../../client-library-state.svelte.js';
	import { ClientVersionState, setClientVersionState } from '../../client-version-state.svelte.js';
	const versions = setClientVersionState(new ClientVersionState());
	import ClientInstallProgressDialog from './client-install-progress-dialog.svelte';
	let { children }: { children: Snippet } = $props();
	const library = setClientLibraryState(new ClientLibraryState());
	const installs = setClientInstallState(new ClientInstallState((receipt) => library.acceptCompleted(receipt)));
	onMount(() => { void installs.sync(); return () => { installs.dispose(); library.dispose(); versions.dispose(); }; });
</script>
{@render children()}
<ClientInstallProgressDialog />
