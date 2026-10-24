<script lang="ts">
	import { accountSkin } from '../account-skin';
	import type { AccountSkin, AccountSummary } from '../runtime';
	let { account, size = 28 }: { account: AccountSummary; size?: number } = $props();
	let skin = $state<AccountSkin | null>(null);
	let loaded = $state(false);
	$effect(() => {
		const id = account.id;
		let active = true;
		skin = null;
		loaded = false;
		void accountSkin(id).then(value => { if (active) skin = value; });
		return () => { active = false; };
	});
</script>

<span class="relative inline-flex shrink-0 items-center justify-center overflow-hidden rounded-md bg-chrome-selected text-foreground" style:width={`${size}px`} style:height={`${size}px`} title={skin && loaded ? `${account.username} · ${skin.model === 'slim' ? '纤细' : '标准'}皮肤` : account.username} aria-hidden="true">
	{#if !loaded}<span class="text-xs font-medium">{account.username.slice(0, 1).toUpperCase()}</span>{/if}
	{#if skin}
		<img src={skin.url} alt="" referrerpolicy="no-referrer" class="pointer-events-none absolute max-w-none [image-rendering:pixelated]" style:width="800%" style:left="-100%" style:top="-100%" style:opacity={loaded ? 1 : 0} onload={event => { const image = event.currentTarget; loaded = image instanceof HTMLImageElement && image.naturalWidth === 64 && [32, 64].includes(image.naturalHeight); }} onerror={() => { loaded = false; }} />
		{#if loaded}<img src={skin.url} alt="" referrerpolicy="no-referrer" class="pointer-events-none absolute max-w-none [image-rendering:pixelated]" style:width="800%" style:left="-500%" style:top="-100%" />{/if}
	{/if}
</span>
