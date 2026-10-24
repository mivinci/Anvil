<script lang="ts">
	import { isColor } from '../../appearance';
	let { label, value, onChange }: { label: string; value: string; onChange: (value: string) => void } = $props();
	let invalid = $state(false);
	function commit(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		invalid = !isColor(input.value);
		if (!invalid) onChange(input.value.toLowerCase());
	}
</script>
<div>
	<div class="flex h-7 items-center gap-1.5 rounded-md border border-border bg-main-pane px-2 focus-within:ring-1 focus-within:ring-ring">
		<label class="relative size-3.5 shrink-0 overflow-hidden rounded-full border border-border" style:background-color={value}>
			<span class="sr-only">{label}：选择颜色</span>
			<input type="color" {value} oninput={(event) => { invalid = false; onChange(event.currentTarget.value); }} class="absolute inset-0 size-full cursor-pointer opacity-0" />
		</label>
		<input type="text" aria-label={`${label}十六进制色值`} aria-invalid={invalid} {value} maxlength={7} onchange={commit} class="w-16 bg-transparent font-mono text-xs text-foreground outline-none" spellcheck="false" />
	</div>
	{#if invalid}<p role="alert" class="mt-1 text-[11px] text-destructive">请输入 #RRGGBB 色值</p>{/if}
</div>
