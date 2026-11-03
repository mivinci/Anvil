/** Stable visual identity only; this hash is not used for security or content identity. */
export function instanceAvatarPattern(id: string): { hue: number; cells: boolean[] } {
	let seed = 2166136261;
	for (const character of id) seed = Math.imul(seed ^ character.codePointAt(0)!, 16777619) >>> 0;
	const hue = seed % 360;
	const cells: boolean[] = [];
	for (let row = 0; row < 5; row++) {
		const half: boolean[] = [];
		for (let column = 0; column < 3; column++) {
			seed ^= seed << 13; seed ^= seed >>> 17; seed ^= seed << 5;
			half.push((seed >>> 0) % 2 === 0);
		}
		cells.push(...half, half[1]!, half[0]!);
	}
	// Keep even a sparse seed recognizable at small sizes.
	cells[12] = true;
	return { hue, cells };
}
