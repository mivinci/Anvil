<script lang="ts">
	import { onMount, type Snippet } from 'svelte';
	import { AccountState, setAccountState } from '../account-state.svelte.js';
	import AccountLoginDialog from './account-login-dialog.svelte';

	let { children }: { children: Snippet } = $props();
	const accounts = setAccountState(new AccountState());

	onMount(() => {
		void accounts.refreshAccounts();
		return () => accounts.dispose();
	});
</script>

{@render children()}
<AccountLoginDialog />
