import { defaultLocale, messages, type Locale } from './i18n/index.ts';
import type { PlayStatsAccount, PlayStatsSnapshot } from './runtime';

/** Pure presentation helpers shared by the player card and its tests. */

export function playStatsAccount(
	snapshot: PlayStatsSnapshot | null,
	accountId: string | null | undefined,
): PlayStatsAccount | null {
	if (!snapshot || !accountId) return null;
	return snapshot.accounts[accountId] ?? null;
}

export function formatPlayDuration(milliseconds: number, locale: Locale = defaultLocale): string {
	if (!Number.isFinite(milliseconds) || milliseconds <= 0) return '—';
	if (milliseconds < 60_000) return messages[locale].home.lessMinute;
	const minutes = Math.floor(milliseconds / 60_000);
	if (minutes < 60) return messages[locale].home.minutes(minutes, new Intl.NumberFormat(locale).format(minutes));
	const hours = minutes / 60;
	const count = hours < 10 ? Math.round(hours * 10) / 10 : Math.round(hours);
	return messages[locale].home.hours(count, new Intl.NumberFormat(locale, { minimumFractionDigits: hours < 10 ? 1 : 0, maximumFractionDigits: hours < 10 ? 1 : 0 }).format(count));
}

export function formatPlayMoment(value: number | null | undefined, now = Date.now(), locale: Locale = defaultLocale): string {
	if (!value || !Number.isFinite(value) || value <= 0) return messages[locale].home.noRecord;
	const elapsed = Math.max(0, now - value);
	const minutes = Math.floor(elapsed / 60_000);
	if (minutes < 1) return messages[locale].home.justNow;
	if (minutes < 60) return new Intl.RelativeTimeFormat(locale, { numeric: 'always' }).format(-minutes, 'minute');
	const hours = Math.floor(minutes / 60);
	if (hours < 24) return new Intl.RelativeTimeFormat(locale, { numeric: 'always' }).format(-hours, 'hour');
	const days = Math.floor(hours / 24);
	if (days < 30) return new Intl.RelativeTimeFormat(locale, { numeric: 'always' }).format(-days, 'day');
	return new Intl.DateTimeFormat(locale, { year: 'numeric', month: 'short', day: 'numeric' }).format(value);
}

export function formatPlayCount(count: number, locale: Locale = defaultLocale): string {
	if (!Number.isSafeInteger(count) || count <= 0) return '—';
	return messages[locale].home.playCount(count, new Intl.NumberFormat(locale).format(count));
}

/** Most-played instance ID; ties fall back to the most recent session. */
export function mostPlayedInstance(account: PlayStatsAccount | null): string | null {
	if (!account) return null;
	const entries = Object.entries(account.instances);
	entries.sort((left, right) => right[1].totalMs - left[1].totalMs || right[1].lastPlayedAt - left[1].lastPlayedAt);
	return entries[0]?.[0] ?? null;
}
