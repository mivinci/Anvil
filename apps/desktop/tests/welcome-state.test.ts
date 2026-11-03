import assert from 'node:assert/strict';
import { test } from 'node:test';
import { WelcomeState, welcomeStorageKey } from '../src/lib/welcome-state.ts';

function storage(initial: Record<string, string> = {}) {
	const values = new Map(Object.entries(initial));
	return { values, getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => { values.set(key, value); } };
}

test('new installation records pending before preferences and completion survives restart', () => {
	const saved = storage();
	const welcome = new WelcomeState(saved, 'empty');
	assert.equal(welcome.visible, true);
	assert.equal(saved.getItem(welcomeStorageKey), 'pending');
	saved.setItem('anvil.theme', 'fixture');
	assert.equal(new WelcomeState(saved, 'empty').visible, true);
	welcome.complete();
	assert.equal(welcome.visible, false);
	assert.equal(new WelcomeState(saved, 'empty').visible, false);
});

test('existing preferences or Runtime data skip welcome without modifying preferences', () => {
	for (const key of ['anvil.locale', 'anvil.appearance', 'anvil.theme', 'anvil.recent-plays']) {
		const saved = storage({ [key]: 'fixture' });
		assert.equal(new WelcomeState(saved, 'unknown').visible, false);
		assert.equal(saved.getItem(key), 'fixture');
		assert.equal(saved.getItem(welcomeStorageKey), 'complete');
	}
	const saved = storage();
	assert.equal(new WelcomeState(saved, 'present').visible, false);
	assert.equal(saved.getItem(welcomeStorageKey), 'complete');
});

test('unconfirmed Runtime data is not classified as a new installation', () => {
	const saved = storage();
	assert.equal(new WelcomeState(saved, 'unknown').visible, false);
	assert.equal(saved.getItem(welcomeStorageKey), null);
});

test('storage failures cannot trap users in welcome or overwrite preferences', () => {
	const unreadable = new WelcomeState({ getItem() { throw new Error('blocked'); }, setItem() { assert.fail('must not write after failed read'); } }, 'empty');
	assert.equal(unreadable.visible, false);
	assert.equal(unreadable.storageFailed, true);
	const unwritable = new WelcomeState({ getItem() { return null; }, setItem() { throw new Error('blocked'); } }, 'empty');
	assert.equal(unwritable.visible, true);
	assert.equal(unwritable.storageFailed, true);
	unwritable.complete();
	assert.equal(unwritable.visible, false);
});

test('unknown marker preserves existing data and does not bypass first-use checks', () => {
	assert.equal(new WelcomeState(storage({ [welcomeStorageKey]: 'future' }), 'empty').visible, true);
	assert.equal(new WelcomeState(storage({ [welcomeStorageKey]: 'future' }), 'present').visible, false);
});
