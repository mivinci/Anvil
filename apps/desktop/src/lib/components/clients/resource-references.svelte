<script lang="ts">
	import { localeState } from '#lib/locale.svelte.ts';
	import { listResourceReferences, type ResourceReference } from '../../runtime';
	import { requestErrorMessage } from '../../runtime-errors.ts';

	let { sha256 }: { sha256: string } = $props();
	const copy = $derived(localeState.messages.mods.discovery);
	let references = $state<ResourceReference[] | null>(null);
	let error = $state<unknown>(null);
	let busy = $state(false);
	let request: AbortController | null = null;
	$effect(() => {
		sha256;
		references = null;
		error = null;
		busy = false;
		return () => { request?.abort(); request = null; };
	});
	async function load() {
		if (busy) return;
		const controller = new AbortController();
		request = controller;
		busy = true;
		error = null;
		references = null;
		try {
			const result = await listResourceReferences(sha256, AbortSignal.any([controller.signal, AbortSignal.timeout(15_000)]));
			if (request === controller) references = result;
		} catch (reason) {
			if (request === controller) error = reason;
		} finally {
			if (request === controller) { request = null; busy = false; }
		}
	}
</script>

<div class="mt-2 border-t border-border pt-2 text-xs">
	<button type="button" onclick={() => void load()} disabled={busy} class="rounded-md px-1.5 py-1 text-primary hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40">{copy.findReferences}</button>
	{#if busy}<p role="status" class="mt-1 text-muted-foreground">{localeState.messages.common.loading}</p>{/if}
	{#if error}<p role="alert" class="mt-1 break-words text-destructive">{requestErrorMessage(error, localeState.current, copy.referencesFailed)}</p>{/if}
	{#if references}
		{#if references.length === 0}<p class="mt-1 text-muted-foreground">{copy.noReferences}</p>
		{:else}
			<ul class="mt-1 max-h-36 space-y-1 overflow-y-auto overscroll-contain">
				{#each references as entry (`${entry.instanceId}:${entry.kind}:${entry.filename}`)}
					<li class="break-words rounded-md bg-card px-2 py-1">
						<span class="font-medium">{entry.instanceName}</span>
						<span class="text-muted-foreground"> · {entry.kind === 'mod' ? copy.referenceMod : entry.kind === 'resource_pack' ? copy.referencePack : copy.referenceShader}{#if entry.enabled !== null} · {entry.enabled ? localeState.messages.instance.detail.enabled : localeState.messages.instance.detail.disabled}{/if}</span>
						<p class="text-muted-foreground">{entry.filename}</p>
					</li>
				{/each}
			</ul>
		{/if}
	{/if}
</div>
