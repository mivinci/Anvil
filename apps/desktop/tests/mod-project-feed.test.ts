import { test } from 'node:test';
import assert from 'node:assert/strict';
import { ModProjectFeed, type ModProjectFeedSnapshot } from '../src/lib/mod-catalog-read.ts';

const page = (ids: string[], nextIndex: number | null) => ({
	items: ids.map(id => ({ id, name: id, summary: '', authors: [], links: { websiteUrl: null } })),
	nextIndex, revision: 1, freshForMs: 1000, stale: false, refreshError: null as string | null,
});
const flush = () => new Promise(resolve => setImmediate(resolve));
function harness() {
	let current: ModProjectFeedSnapshot;
	const feed = new ModProjectFeed(value => { current = value; });
	return { feed, get value() { return current!; } };
}

test('scroll appends the actual cursor once and deduplicates overlapping results', async () => {
	const h = harness(), cursors: number[] = [];
	let finish!: (value: ReturnType<typeof page>) => void;
	h.feed.reset(async cursor => {
		cursors.push(cursor);
		return cursor === 0 ? page(['a', 'b'], 37) : new Promise(resolve => { finish = resolve; });
	});
	await flush();
	const pending = h.feed.more();
	await h.feed.more();
	assert.deepEqual(cursors, [0, 37]);
	finish(page(['b', 'c'], null));
	await pending;
	assert.deepEqual(h.value.items.map(item => item.id), ['a', 'b', 'c']);
	await h.feed.more();
	assert.deepEqual(cursors, [0, 37]);
});

test('failed append keeps previous results, blocks automatic retries and retries the same cursor', async () => {
	const h = harness(), cursors: number[] = [];
	let fail = true;
	h.feed.reset(async cursor => {
		cursors.push(cursor);
		if (cursor === 0) return page(['a'], 20);
		if (fail) throw new Error('offline');
		return page(['b'], null);
	});
	await flush(); await h.feed.more(); await h.feed.more();
	assert.deepEqual(h.value.items.map(item => item.id), ['a']);
	assert.equal(h.value.error, 'offline');
	assert.deepEqual(cursors, [0, 20]);
	fail = false; await h.feed.retry();
	assert.deepEqual(cursors, [0, 20, 20]);
	assert.deepEqual(h.value.items.map(item => item.id), ['a', 'b']);
});

test('query reset cancels pending append and discards its late response', async () => {
	const h = harness();
	let finish!: (value: ReturnType<typeof page>) => void;
	let oldSignal!: AbortSignal;
	h.feed.reset(async (cursor, signal) => {
		if (cursor === 0) return page(['old'], 20);
		oldSignal = signal;
		return new Promise(resolve => { finish = resolve; });
	});
	await flush(); const pending = h.feed.more();
	h.feed.reset(async () => page(['new'], null));
	assert.equal(oldSignal.aborted, true);
	finish(page(['late'], null)); await pending; await flush();
	assert.deepEqual(h.value.items.map(item => item.id), ['new']);
	assert.equal(h.value.error, null);
});

test('failed forced refresh keeps all pages; successful refresh replaces the old sequence', async () => {
	const h = harness();
	let fail = false;
	h.feed.reset(async (cursor, _signal, options) => {
		if (options.refresh) {
			if (fail) return { ...page(['a'], 20), stale: true, refreshError: 'offline' };
			return page(['fresh'], null);
		}
		return cursor === 0 ? page(['a'], 20) : page(['b'], null);
	});
	await flush(); await h.feed.more(); fail = true; await h.feed.refresh();
	assert.deepEqual(h.value.items.map(item => item.id), ['a', 'b']);
	assert.equal(h.value.warning, 'offline');
	fail = false; await h.feed.retry();
	assert.deepEqual(h.value.items.map(item => item.id), ['fresh']);
});

test('non-advancing or out-of-range cursors stop rather than looping', async () => {
	for (const cursor of [0, -1, 10000]) {
		const h = harness(); let calls = 0;
		h.feed.reset(async () => { calls++; return page(['a'], cursor); });
		await flush(); await h.feed.more();
		assert.equal(calls, 1);
		assert.equal(h.value.nextIndex, null);
	}
});
