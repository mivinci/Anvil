<script lang="ts">
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import { localeState } from '#lib/locale.svelte.ts';
</script>

<svelte:head><title>{localeState.messages.common.pageTitle(localeState.messages.errors.pageUnavailable)}</title></svelte:head>

<section class="flex min-h-0 flex-1 flex-col items-center justify-center gap-3 px-6 py-10 text-center">
	<h1 class="text-lg font-semibold">{localeState.messages.errors.pageUnavailable}</h1>
	<p class="text-body text-muted-foreground">{page.status === 404 ? localeState.messages.errors.notFound : localeState.messages.errors.pageFailed}</p>
	<div class="flex flex-wrap justify-center gap-2">
		<button type="button" onclick={() => location.reload()} class="rounded-md bg-primary px-3 py-1.5 text-xs text-primary-foreground focus-visible:outline-2 focus-visible:outline-ring">{localeState.messages.common.retry}</button>
		<a href={resolve('/')} class="rounded-md px-3 py-1.5 text-xs hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring">{localeState.messages.common.returnHome}</a>
	</div>
	{#if page.error?.message}
		<details class="max-w-full text-left text-xs text-muted-foreground"><summary class="cursor-pointer">{localeState.messages.errors.technicalDetails}</summary><pre class="mt-2 max-h-48 overflow-auto whitespace-pre-wrap break-words">{page.error.message}</pre></details>
	{/if}
</section>
