import type { Locale } from './index.ts';

export function formatCount(value: number, locale: Locale, options: Intl.NumberFormatOptions = {}): string {
	return new Intl.NumberFormat(locale, options).format(value);
}

/** Select decimal or binary explicitly when migrating existing formatters. */
export function formatBytes(value: number, locale: Locale, options: { base?: 1000 | 1024; maximumFractionDigits?: number } = {}): string {
	const { base = 1024, maximumFractionDigits = 1 } = options;
	const units = base === 1024 ? ['B', 'KiB', 'MiB', 'GiB', 'TiB', 'PiB'] : ['B', 'KB', 'MB', 'GB', 'TB', 'PB'];
	const absolute = Math.abs(value);
	const exponent = Number.isFinite(absolute) && absolute >= 1
		? Math.min(Math.floor(Math.log(absolute) / Math.log(base)), units.length - 1) : 0;
	return `${formatCount(value / base ** exponent, locale, { maximumFractionDigits })} ${units[exponent]}`;
}

export function formatDate(value: Date | number, locale: Locale, options: Intl.DateTimeFormatOptions = {}): string {
	return new Intl.DateTimeFormat(locale, { year: 'numeric', month: 'short', day: 'numeric', ...options }).format(value);
}

export function formatDateTime(value: Date | number, locale: Locale, options: Intl.DateTimeFormatOptions = {}): string {
	return new Intl.DateTimeFormat(locale, { year: 'numeric', month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit', ...options }).format(value);
}

export function plural(value: number, locale: Locale, forms: { one: string; other: string }): string {
	return new Intl.PluralRules(locale).select(value) === 'one' ? forms.one : forms.other;
}
