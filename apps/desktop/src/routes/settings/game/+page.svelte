<script lang="ts">
	import { localeState } from '#lib/locale.svelte.ts';
	const text = $derived(localeState.messages.settings);
</script>
<div class="rounded-xl bg-card p-4"><p class="text-body font-medium">{text.unavailable}</p><p class="mt-2 text-xs leading-5 text-muted-foreground">{text.gamePending}</p></div>
