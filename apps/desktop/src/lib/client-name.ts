/** Match Core name validation; the Runtime remains authoritative for identity/existence. */
export function validClientName(name: string): boolean {
	if (!/^[\p{Alphabetic}\p{Number}][\p{Alphabetic}\p{Number}._-]*$/u.test(name) || name.endsWith('.')) return false;
	return /^[\x00-\x7f]*$/.test(name) ? name.length <= 48 : new TextEncoder().encode(name).length <= 128;
}
