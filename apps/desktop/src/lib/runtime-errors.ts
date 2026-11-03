import { defaultLocale, messages, type Locale } from './i18n/index.ts';

/** Missing codes mean an older Runtime. Unknown codes never become primary text. */
export function runtimeErrorMessage(code: string | null | undefined, locale: Locale = defaultLocale, fallback?: string): string {
	if (!code) return fallback ?? messages[locale].errors.unknown;
	const copy = messages[locale].errors.runtime;
	return Object.hasOwn(copy, code) ? copy[code as keyof typeof copy] : messages[locale].errors.unknown;
}
export function runtimeErrorCode(error: unknown): string | null {
	if (!error || typeof error !== 'object' || !('code' in error)) return null;
	return typeof error.code === 'string' && /^[a-z][a-z0-9_]{0,63}$/.test(error.code) ? error.code : null;
}
export function requestErrorMessage(error: unknown, locale: Locale, fallback: string): string {
	return runtimeErrorMessage(runtimeErrorCode(error), locale, fallback);
}
