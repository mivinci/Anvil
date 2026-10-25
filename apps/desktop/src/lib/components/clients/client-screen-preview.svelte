<script lang="ts">
	import { Play } from '@lucide/svelte';
</script>

<figure aria-label="游戏画面预览占位，尚未接入截图或实时画面" class="w-full select-none">
	<div class="flex aspect-video flex-col items-center justify-center gap-1.5 overflow-hidden rounded-md bg-card/60">
		<Play size={22} strokeWidth={1.25} class="text-muted-foreground/50" aria-hidden="true" />
		<span class="text-[11px] text-muted-foreground">暂无游戏画面</span>
	</div>
</figure>
