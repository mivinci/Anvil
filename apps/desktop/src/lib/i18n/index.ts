import { zhCN } from './zh-CN.ts';
import { en } from './en.ts';
export type { Messages } from './zh-CN.ts';

export const locales = ['zh-CN', 'en'] as const;
export type Locale = (typeof locales)[number];
export const defaultLocale: Locale = 'zh-CN';
export const localeStorageKey = 'anvil.locale';
export const localeNames: Record<Locale, string> = { 'zh-CN': '简体中文', en: 'English' };
export const messages = { 'zh-CN': zhCN, en };

export function isLocale(value: unknown): value is Locale {
	return value === 'zh-CN' || value === 'en';
}

export function resolveLocale(saved: unknown, languages: readonly string[] = [], language = ''): Locale {
	if (isLocale(saved)) return saved;
	for (const candidate of languages.length ? languages : [language]) {
		const primary = candidate.toLowerCase().split('-')[0];
		if (primary === 'en') return 'en';
		if (primary === 'zh') return 'zh-CN';
	}
	return defaultLocale;
}

export { formatBytes, formatCount, formatDate, formatDateTime, plural } from './format.ts';
