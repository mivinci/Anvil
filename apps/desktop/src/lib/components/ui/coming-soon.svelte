<script lang="ts">
	import { localeState } from '#lib/locale.svelte.ts';
	import { Construction } from '@lucide/svelte';
</script>

<div class="flex min-h-48 flex-1 flex-col items-center justify-center px-6 py-10 text-center">
	<Construction size={28} strokeWidth={1.4} class="mb-4 text-muted-foreground/50" aria-hidden="true" />
	<p class="text-[13px] font-medium">{localeState.messages.common.notAvailable}</p>
	<p class="mt-1.5 text-[11px] text-muted-foreground">{localeState.messages.common.comingSoon}</p>
</div>
