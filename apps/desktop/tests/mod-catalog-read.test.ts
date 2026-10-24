import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readModCatalog } from '../src/lib/mod-catalog-read.ts';

const snapshot = (revision: number, stale = false, refreshError: string | null = null) => ({ revision, stale, refreshError, freshForMs: 0 });

test('expired snapshot is visible before joining the existing refresh revision', async () => {
	const published: number[] = [];
	const options: unknown[] = [];
	await readModCatalog(async option => {
		options.push(option);
		if (options.length === 1) return snapshot(7, true);
		assert.deepEqual(published, [7]);
		return snapshot(8);
	}, value => published.push(value.revision), new AbortController().signal);
	assert.deepEqual(options, [{ refresh: false }, { refresh: true, after: 7 }]);
	assert.deepEqual(published, [7, 8]);
});

test('failed refresh retains the published snapshot and never polls', async () => {
	const published: number[] = [];
	let calls = 0;
	await assert.rejects(readModCatalog(async () => {
		if (++calls === 1) return snapshot(7, true);
		throw new Error('offline');
	}, value => published.push(value.revision), new AbortController().signal), /offline/);
	assert.deepEqual(published, [7]);
	assert.equal(calls, 2);
});

test('a late response from a cancelled context cannot publish or start a refresh', async () => {
	const controller = new AbortController();
	let published = false;
	await assert.rejects(readModCatalog(async () => {
		controller.abort();
		return snapshot(1, true);
	}, () => { published = true; }, controller.signal), { name: 'AbortError' });
	assert.equal(published, false);
});

test('fresh, failed-refresh and manual-refresh snapshots each finish in one request', async () => {
	for (const [value, force] of [[snapshot(1), false], [snapshot(2, true, 'offline'), false], [snapshot(3, true), true]] as const) {
		let calls = 0;
		await readModCatalog(async options => {
			calls++;
			assert.equal(options.refresh, force);
			return value;
		}, () => {}, new AbortController().signal, force);
		assert.equal(calls, 1);
	}
});
