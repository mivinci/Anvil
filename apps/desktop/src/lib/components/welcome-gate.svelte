<script lang="ts">
	import { onMount, type Snippet } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { localeState } from '#lib/locale.svelte.ts';
	import { listAccounts, listInstalledClients } from '../runtime';
	import { WelcomeState, type ExistingWelcomeData } from '../welcome-state';
	import WelcomeScreen from './welcome-screen.svelte';

	let { children }: { children: Snippet } = $props();
	let checking = $state(true);
	let visible = $state(false);
	let storageFailed = $state(false);
	let dismissedNotice = $state(false);
	let welcome: WelcomeState;

	onMount(() => {
		const cancel = new AbortController();
		const storage = {
			getItem: (key: string) => window.localStorage.getItem(key),
			setItem: (key: string, value: string) => window.localStorage.setItem(key, value),
		};
		async function initialize() {
			welcome = new WelcomeState(storage, 'unknown');
			if (welcome.requiresData) {
				const signal = AbortSignal.any([cancel.signal, AbortSignal.timeout(5000)]);
				const results = await Promise.allSettled([listAccounts({ signal }), listInstalledClients({ signal })]);
				if (cancel.signal.aborted) return;
				const lists = results.map(result => result.status === 'fulfilled' && Array.isArray(result.value) ? result.value : null);
				const existing: ExistingWelcomeData = lists.some(list => list && list.length > 0) ? 'present'
					: lists.every(list => list !== null && list.length === 0) ? 'empty' : 'unknown';
				welcome = new WelcomeState(storage, existing);
			}
			visible = welcome.visible;
			storageFailed = welcome.storageFailed;
			checking = false;
		}
		void initialize();
		return () => cancel.abort();
	});

	async function complete() {
		// Welcome is window-wide even if the application was opened on a deep link.
		await goto(resolve('/'), { replace: true });
		welcome.complete();
		storageFailed = welcome.storageFailed;
		visible = false;
	}
</script>

{#if checking || visible}
	<header class="h-8 shrink-0" data-tauri-drag-region aria-label={localeState.messages.common.dragRegion}></header>
	{#if checking}
		<div class="grid min-h-0 flex-1 place-content-center text-foreground"><p role="status" class="text-xs text-muted-foreground">{localeState.messages.common.preparing}</p></div>
	{:else}
		<WelcomeScreen oncomplete={() => void complete()} {storageFailed} />
	{/if}
{:else}
	{@render children()}
	{#if storageFailed && !dismissedNotice}
		<div role="status" class="fixed bottom-4 left-1/2 z-50 flex w-max max-w-[calc(100%-2rem)] -translate-x-1/2 items-center gap-3 rounded-xl bg-popover px-4 py-3 text-xs text-popover-foreground shadow-lg">
			<p>{localeState.messages.welcome.storageFailed}</p>
			<button type="button" onclick={() => dismissedNotice = true} class="shrink-0 rounded px-2 py-1 hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring">{localeState.messages.common.close}</button>
		</div>
	{/if}
{/if}
