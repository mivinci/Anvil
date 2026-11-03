<script lang="ts">
	import { localeState } from '#lib/locale.svelte.ts';
	import ComingSoon from '#lib/components/ui/coming-soon.svelte';
</script>

<svelte:head><title>{localeState.messages.common.pageTitle(localeState.messages.nav.servers)}</title></svelte:head>

<h1 class="sr-only">{localeState.messages.nav.servers}</h1>
<section aria-label={localeState.messages.nav.servers} class="flex min-h-0 flex-1 flex-col overflow-y-auto overscroll-contain">
	<ComingSoon />
</section>
