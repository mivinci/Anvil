<script lang="ts">
	import { localeState } from '#lib/locale.svelte.ts';
	import { Play } from '@lucide/svelte';
</script>

<figure aria-label={localeState.messages.instance.detail.previewHint} class="w-full select-none">
	<div class="flex aspect-video flex-col items-center justify-center gap-1.5 overflow-hidden rounded-md bg-card/60">
		<Play size={22} strokeWidth={1.25} class="text-muted-foreground/50" aria-hidden="true" />
		<span class="text-xs text-muted-foreground">{localeState.messages.instance.detail.noPreview}</span>
	</div>
</figure>
