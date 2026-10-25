import { getAccountSkin, type AccountSkin } from './runtime';

const cache = new Map<string, { expires: number; value: Promise<AccountSkin | null> }>();

export function accountSkin(id: string): Promise<AccountSkin | null> {
	const previous = cache.get(id);
	if (previous && previous.expires > Date.now()) return previous.value;
	const entry = { expires: Date.now() + 5 * 60_000, value: Promise.resolve<AccountSkin | null>(null) };
	entry.value = getAccountSkin(id, AbortSignal.timeout(12_000)).then(skin => {
		if (!skin) return null;
		const url = new URL(skin.url);
		if (url.protocol !== 'https:' || url.hostname !== 'textures.minecraft.net' || url.port || url.username || url.password || url.search || url.hash || !/^\/texture\/[a-f0-9]{64}$/i.test(url.pathname) || !['classic', 'slim'].includes(skin.model)) return null;
		return skin;
	}).catch(() => { entry.expires = Date.now() + 60_000; return null; });
	cache.delete(id);
	if (cache.size >= 16) cache.delete(cache.keys().next().value!);
	cache.set(id, entry);
	return entry.value;
}
