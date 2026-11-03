import assert from 'node:assert/strict';
import { test } from 'node:test';
import { instanceAvatarPattern } from '../src/lib/instance-avatar.ts';

test('instance identity remains stable across renders and Unicode IDs', () => {
	for (const id of ['', 'client-first', '海岛生存', '🌴']) {
		assert.deepEqual(instanceAvatarPattern(id), instanceAvatarPattern(id));
	}
	assert.notDeepEqual(instanceAvatarPattern('client-first'), instanceAvatarPattern('client-second'));
});

test('default avatars form bounded symmetric grids with a visible center', () => {
	for (let index = 0; index < 100; index++) {
		const { hue, cells } = instanceAvatarPattern(`client-${index}`);
		assert.ok(hue >= 0 && hue < 360);
		assert.equal(cells.length, 25);
		assert.equal(cells[12], true);
		for (let row = 0; row < 5; row++) for (let column = 0; column < 5; column++) {
			assert.equal(cells[row * 5 + column], cells[row * 5 + 4 - column]);
		}
	}
});
