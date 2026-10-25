export function formatModDownloads(value: number | null | undefined): string | null {
	if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 0) return null;
	return new Intl.NumberFormat('zh-CN', { notation: 'compact', maximumFractionDigits: 1 }).format(value);
}

export function formatModUpdated(value: string | null | undefined): string | null {
	if (!value || value.length > 64 || !/^\d{4}-\d{2}-\d{2}T.*(?:Z|[+-]\d{2}:\d{2})$/.test(value)) return null;
	const date = new Date(value);
	return Number.isFinite(date.getTime()) ? date.toISOString().slice(0, 10) : null;
}
