<script lang="ts">
	import type { Snippet } from 'svelte';
	let { src, alt, class: className = '', fallback }: {
		src?: string | null; alt: string; class?: string; fallback: Snippet;
	} = $props();
	let failedSource = $state<string | null>(null);
</script>

<span role="img" aria-label={alt} class={`relative inline-grid shrink-0 overflow-hidden rounded-xl ${className}`}>
	{#if src && src !== failedSource}
		<img {src} alt="" aria-hidden="true" class="size-full object-cover" loading="lazy" decoding="async" onerror={(event) => failedSource = event.currentTarget.getAttribute('src')} />
	{:else}
		<span aria-hidden="true" class="contents">{@render fallback()}</span>
	{/if}
</span>
