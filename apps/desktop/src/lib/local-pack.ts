import { messages, defaultLocale, type Locale } from './i18n/index.ts';
/** Shared local ZIP limits and file presentation for resource packs and shaders. */
export const MAX_LOCAL_PACK_BYTES = 64 * 1024 * 1024;
export interface LocalPackEntry { source: string; filename: string; sha256: string; size: number }
export interface LocalPackCollection { revision: number; appliedRevision: number | null; packs: LocalPackEntry[] }
export function localPackFileIssue(file: Pick<File, 'name' | 'size'>): 'invalidZip' | 'invalidZipName' | null {
	if (!file.name.toLowerCase().endsWith('.zip') || file.size === 0 || file.size > MAX_LOCAL_PACK_BYTES) {
		return 'invalidZip';
	}
	if (!/^[a-z0-9][a-z0-9._+ -]*\.zip$/i.test(file.name) || file.name.length > 128 || /^(con|prn|aux|nul|com\d|lpt\d)(\.|$)/i.test(file.name)) {
		return 'invalidZipName';
	}
	return null;
}
export function localPackFileError(file: Pick<File, 'name' | 'size'>, locale: Locale = defaultLocale): string | null {
	const issue = localPackFileIssue(file);
	return issue ? messages[locale].instance.detail[issue] : null;
}
export function localPackSize(bytes: number, locale: Locale = defaultLocale) {
	return bytes < 1024 * 1024 ? `${new Intl.NumberFormat(locale, { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(bytes / 1024)} KiB` : `${new Intl.NumberFormat(locale, { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(bytes / 1024 / 1024)} MiB`;
}
export function validateLocalPacks<T extends LocalPackCollection>(value: T): T {
	if (!value || !Number.isSafeInteger(value.revision) || value.revision < 0
		|| !(value.appliedRevision === null || (Number.isSafeInteger(value.appliedRevision) && value.appliedRevision >= 0 && value.appliedRevision <= value.revision))
		|| !Array.isArray(value.packs) || value.packs.length > 1024) throw new Error(messages[defaultLocale].instance.detail.invalidResourceList);
	const hashes = new Set<string>(), names = new Set<string>();
	for (const pack of value.packs) {
		if (!pack || pack.source !== 'local' || typeof pack.filename !== 'string'
			|| localPackFileError({ name: pack.filename, size: pack.size }) !== null
			|| typeof pack.sha256 !== 'string' || !/^[a-f0-9]{64}$/.test(pack.sha256)
			|| !Number.isSafeInteger(pack.size) || pack.size < 1 || hashes.has(pack.sha256) || names.has(pack.filename.toLowerCase())) throw new Error(messages[defaultLocale].instance.detail.invalidResourceEntry);
		hashes.add(pack.sha256); names.add(pack.filename.toLowerCase());
	}
	return value;
}
