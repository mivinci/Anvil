<script lang="ts">
	import InstanceAvatar from './instance-avatar.svelte';
	import { getClientAvatarState } from '../../client-avatar-state.svelte.js';
	let { id, name, class: className = 'size-14' }: { id: string; name: string; class?: string } = $props();
	const avatars = getClientAvatarState();
	const view = $derived(avatars.view(name));
	$effect(() => { void avatars.ensure(name); });
</script>
<InstanceAvatar {id} {name} src={view.url} class={className} />
