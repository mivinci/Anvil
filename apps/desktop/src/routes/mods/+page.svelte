<script lang="ts">
	import { localeState } from '#lib/locale.svelte.ts';
	import ModDiscoveryShell from '#lib/components/mods/mod-discovery-shell.svelte';
</script>

<svelte:head><title>{localeState.messages.common.pageTitle(localeState.messages.nav.mods)}</title></svelte:head>

<ModDiscoveryShell />
