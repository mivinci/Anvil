export interface RecentPlay { name: string; playedAt: number }
const key = 'anvil.recent-plays';
export function clearRecentPlays(): boolean {
	try { localStorage.removeItem(key); return true; }
	catch { return false; }
}

export function forgetRecentPlay(previous: RecentPlay[], name: string): RecentPlay[] {
	const next = previous.filter(value => value.name !== name.toLowerCase());
	try { localStorage.setItem(key, JSON.stringify(next)); } catch { /* Window history still updates. */ }
	return next;
}

export function readRecentPlays(): RecentPlay[] {
	try {
		const raw = localStorage.getItem(key);
		if (!raw || raw.length > 16_384) return [];
		const values: unknown = JSON.parse(raw);
		if (!Array.isArray(values)) return [];
		return values.filter((value): value is RecentPlay => typeof value?.name === 'string' && /^[a-z0-9][a-z0-9._-]{0,63}$/i.test(value.name) && Number.isSafeInteger(value.playedAt) && value.playedAt > 0 && value.playedAt <= Date.now())
			.sort((a, b) => b.playedAt - a.playedAt).filter((value, index, all) => all.findIndex(item => item.name === value.name) === index).slice(0, 12);
	} catch { return []; }
}

export function recordRecentPlay(previous: RecentPlay[], name: string): RecentPlay[] {
	const next = [{ name: name.toLowerCase(), playedAt: Date.now() }, ...previous.filter(value => value.name !== name.toLowerCase())].slice(0, 12);
	try { localStorage.setItem(key, JSON.stringify(next)); } catch { /* Keep window history if storage is unavailable. */ }
	return next;
}
