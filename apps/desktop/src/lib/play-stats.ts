import type { PlayStatsAccount, PlayStatsSnapshot } from './runtime';

/** Pure presentation helpers shared by the player card and its tests. */

export function playStatsAccount(
	snapshot: PlayStatsSnapshot | null,
	accountId: string | null | undefined,
): PlayStatsAccount | null {
	if (!snapshot || !accountId) return null;
	return snapshot.accounts[accountId] ?? null;
}

export function formatPlayDuration(milliseconds: number): string {
	if (!Number.isFinite(milliseconds) || milliseconds <= 0) return '—';
	if (milliseconds < 60_000) return '不到 1 分钟';
	const minutes = Math.floor(milliseconds / 60_000);
	if (minutes < 60) return `${minutes} 分钟`;
	const hours = minutes / 60;
	return hours < 10 ? `${hours.toFixed(1)} 小时` : `${Math.round(hours)} 小时`;
}

export function formatPlayMoment(value: number | null | undefined, now = Date.now()): string {
	if (!value || !Number.isFinite(value) || value <= 0) return '尚无记录';
	const elapsed = Math.max(0, now - value);
	const minutes = Math.floor(elapsed / 60_000);
	if (minutes < 1) return '刚刚';
	if (minutes < 60) return `${minutes} 分钟前`;
	const hours = Math.floor(minutes / 60);
	if (hours < 24) return `${hours} 小时前`;
	const days = Math.floor(hours / 24);
	if (days < 30) return `${days} 天前`;
	return new Intl.DateTimeFormat('zh-CN', { year: 'numeric', month: 'short', day: 'numeric' }).format(value);
}

export function formatPlayCount(count: number): string {
	if (!Number.isSafeInteger(count) || count <= 0) return '—';
	return `${count} 次`;
}

/** Most-played instance ID; ties fall back to the most recent session. */
export function mostPlayedInstance(account: PlayStatsAccount | null): string | null {
	if (!account) return null;
	const entries = Object.entries(account.instances);
	entries.sort((left, right) => right[1].totalMs - left[1].totalMs || right[1].lastPlayedAt - left[1].lastPlayedAt);
	return entries[0]?.[0] ?? null;
}
