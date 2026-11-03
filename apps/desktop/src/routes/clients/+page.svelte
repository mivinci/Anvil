<script lang="ts">
	import { localeState } from '#lib/locale.svelte.ts';
	import ClientLibrary from '#lib/components/clients/client-library.svelte';
</script>

<svelte:head><title>{localeState.messages.common.pageTitle(localeState.messages.nav.clients)}</title></svelte:head>
<ClientLibrary />
