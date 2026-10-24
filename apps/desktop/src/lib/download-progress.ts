/** Decimal byte units; network rate is measured separately from staged cache data. */
export function formatBytes(value: number): string {
	if (!Number.isFinite(value) || value <= 0) return '0 B';
	const units = ['B', 'KB', 'MB', 'GB', 'TB'];
	const index = Math.max(0, Math.min(Math.floor(Math.log(value) / Math.log(1_000)), units.length - 1));
	const amount = value / 1_000 ** index;
	return `${amount.toFixed(index === 0 ? 0 : amount < 10 ? 2 : 1)} ${units[index]}`;
}
