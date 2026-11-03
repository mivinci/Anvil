import { defaultLocale, type Locale } from './i18n/index.ts';

export function formatModDownloads(value: number | null | undefined, locale: Locale = defaultLocale): string | null {
	if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 0) return null;
	return new Intl.NumberFormat(locale, { notation: 'compact', maximumFractionDigits: 1 }).format(value);
}

export function formatModUpdated(value: string | null | undefined, locale: Locale = defaultLocale): string | null {
	if (!value || value.length > 64 || !/^\d{4}-\d{2}-\d{2}T.*(?:Z|[+-]\d{2}:\d{2})$/.test(value)) return null;
	const date = new Date(value);
	return Number.isFinite(date.getTime()) ? new Intl.DateTimeFormat(locale, { year: 'numeric', month: 'short', day: 'numeric', timeZone: 'UTC' }).format(date) : null;
}
