import assert from 'node:assert/strict';
import test from 'node:test';
import { clearRecentPlays, readRecentPlays, recordRecentPlay } from '../src/lib/recent-plays.ts';

test('recent plays restore valid history and ignore corrupt storage', () => {
	let saved = 'broken';
	Object.defineProperty(globalThis, 'localStorage', { configurable: true, value: { getItem: () => saved, setItem: (_key: string, value: string) => { saved = value; } } });
	assert.deepEqual(readRecentPlays(), []);
	saved = JSON.stringify([{ name: 'first', playedAt: 1000 }, { name: 'second', playedAt: 2000 }, { name: 'first', playedAt: 3000 }, { name: '../bad', playedAt: 4000 }, { name: 'future', playedAt: Date.now() + 60_000 }]);
	assert.deepEqual(readRecentPlays().map(value => value.name), ['first', 'second']);
	const next = recordRecentPlay(readRecentPlays(), 'Second');
	assert.equal(next[0].name, 'second');
	assert.deepEqual(readRecentPlays().map(value => value.name), ['second', 'first']);
});

test('recent plays stay bounded and survive unavailable local storage in memory', () => {
	Object.defineProperty(globalThis, 'localStorage', { configurable: true, value: { getItem: () => { throw new Error('blocked'); }, setItem: () => { throw new Error('quota'); } } });
	assert.deepEqual(readRecentPlays(), []);
	const previous = Array.from({ length: 12 }, (_, i) => ({ name: `instance-${i}`, playedAt: 1000 }));
	const next = recordRecentPlay(previous, 'new');
	assert.equal(next.length, 12);
	assert.equal(next[0].name, 'new');
});

test('clear removes persisted recent history and reports storage failures', () => {
	let saved: string | null = JSON.stringify([{ name: 'world', playedAt: 1000 }]);
	Object.defineProperty(globalThis, 'localStorage', { configurable: true, value: {
		getItem: () => saved,
		removeItem: (key: string) => { assert.equal(key, 'anvil.recent-plays'); saved = null; },
	} });
	assert.equal(clearRecentPlays(), true);
	assert.deepEqual(readRecentPlays(), []);
	Object.defineProperty(globalThis, 'localStorage', { configurable: true, value: { removeItem: () => { throw new Error('blocked'); } } });
	assert.equal(clearRecentPlays(), false);
});
