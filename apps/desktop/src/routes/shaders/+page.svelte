<script lang="ts">
	import { localeState } from '#lib/locale.svelte.ts';
	import ShaderDiscoveryShell from '#lib/components/shaders/shader-discovery-shell.svelte';
</script>

<svelte:head><title>{localeState.messages.common.pageTitle(localeState.messages.nav.shaders)}</title></svelte:head>

<ShaderDiscoveryShell />
