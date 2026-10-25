/** Shared local ZIP limits and file presentation for resource packs and shaders. */
export const MAX_LOCAL_PACK_BYTES = 64 * 1024 * 1024;
export interface LocalPackEntry { source: string; filename: string; sha256: string; size: number }
export interface LocalPackCollection { revision: number; appliedRevision: number | null; packs: LocalPackEntry[] }
export function localPackFileError(file: Pick<File, 'name' | 'size'>): string | null {
	if (!file.name.toLowerCase().endsWith('.zip') || file.size === 0 || file.size > MAX_LOCAL_PACK_BYTES) {
		return '请选择不超过 64 MiB 的非空 ZIP 文件。';
	}
	if (!/^[a-z0-9][a-z0-9._+ -]*\.zip$/i.test(file.name) || file.name.length > 128 || /^(con|prn|aux|nul|com\d|lpt\d)(\.|$)/i.test(file.name)) {
		return '请调整 ZIP 文件名：以英文字母或数字开头，仅使用英文字母、数字、空格和 . _ + -，最多 128 字符。';
	}
	return null;
}
export function localPackSize(bytes: number) {
	return bytes < 1024 * 1024 ? `${(bytes / 1024).toFixed(1)} KiB` : `${(bytes / 1024 / 1024).toFixed(1)} MiB`;
}
export function validateLocalPacks<T extends LocalPackCollection>(value: T): T {
	if (!value || !Number.isSafeInteger(value.revision) || value.revision < 0
		|| !(value.appliedRevision === null || (Number.isSafeInteger(value.appliedRevision) && value.appliedRevision >= 0 && value.appliedRevision <= value.revision))
		|| !Array.isArray(value.packs) || value.packs.length > 1024) throw new Error('资源清单格式不正确。');
	const hashes = new Set<string>(), names = new Set<string>();
	for (const pack of value.packs) {
		if (!pack || pack.source !== 'local' || typeof pack.filename !== 'string'
			|| localPackFileError({ name: pack.filename, size: pack.size }) !== null
			|| typeof pack.sha256 !== 'string' || !/^[a-f0-9]{64}$/.test(pack.sha256)
			|| !Number.isSafeInteger(pack.size) || pack.size < 1 || hashes.has(pack.sha256) || names.has(pack.filename.toLowerCase())) throw new Error('资源条目格式不正确。');
		hashes.add(pack.sha256); names.add(pack.filename.toLowerCase());
	}
	return value;
}
