import { defaultLocale, type Locale } from './i18n/index.ts';
/** Decimal byte units; network rate is measured separately from staged cache data. */
export function formatBytes(value: number, locale: Locale = defaultLocale): string {
	if (!Number.isFinite(value) || value <= 0) return '0 B';
	const units = ['B', 'KB', 'MB', 'GB', 'TB'];
	const index = Math.max(0, Math.min(Math.floor(Math.log(value) / Math.log(1_000)), units.length - 1));
	const amount = value / 1_000 ** index;
	const digits = index === 0 ? 0 : amount < 10 ? 2 : 1;
	return `${new Intl.NumberFormat(locale, { minimumFractionDigits: digits, maximumFractionDigits: digits }).format(amount)} ${units[index]}`;
}
