<script lang="ts">
	import type { Snippet } from 'svelte';
	import { slide } from 'svelte/transition';
	import { cubicOut } from 'svelte/easing';
	import { MediaQuery } from 'svelte/reactivity';
	let { open, id, label, children }: { open: boolean; id: string; label: string; children: Snippet } = $props();
	const reducedMotion = new MediaQuery('(prefers-reduced-motion: reduce)');
</script>

{#if open}
	<aside {id} aria-label={label} transition:slide={{ axis: 'x', duration: reducedMotion.current ? 0 : 180, easing: cubicOut }} class="flex w-44 shrink-0 flex-col overflow-y-auto overscroll-contain bg-card px-2 py-3">
		<div class="w-40 shrink-0">{@render children()}</div>
	</aside>
{/if}
