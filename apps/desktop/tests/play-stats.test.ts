import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
	formatPlayCount, formatPlayDuration, formatPlayMoment, mostPlayedInstance, playStatsAccount,
} from '../src/lib/play-stats.ts';
import type { PlayStatsAccount, PlayStatsSnapshot } from '../src/lib/runtime.ts';

const account = (overrides: Partial<PlayStatsAccount> = {}): PlayStatsAccount => ({
	totalMs: 0, launches: 0, lastPlayedAt: null, lastInstanceId: null, last7dMs: 0,
	instances: {}, recentSessions: [], ...overrides,
});

test('play durations use minutes up to an hour and hours afterwards', () => {
	assert.equal(formatPlayDuration(0), '—');
	assert.equal(formatPlayDuration(Number.NaN), '—');
	assert.equal(formatPlayDuration(59_999), '不到 1 分钟');
	assert.equal(formatPlayDuration(60_000), '1 分钟');
	assert.equal(formatPlayDuration(90 * 60_000), '1.5 小时');
	assert.equal(formatPlayDuration(9.94 * 3_600_000), '9.9 小时');
	assert.equal(formatPlayDuration(30 * 3_600_000), '30 小时');
});

test('relative play moments stay bounded and readable', () => {
	const now = 1_700_000_000_000;
	assert.equal(formatPlayMoment(null, now), '尚无记录');
	assert.equal(formatPlayMoment(now - 30_000, now), '刚刚');
	assert.equal(formatPlayMoment(now - 5 * 60_000, now), '5 分钟前');
	assert.equal(formatPlayMoment(now - 2 * 3_600_000, now), '2 小时前');
	assert.equal(formatPlayMoment(now - 2 * 86_400_000, now), '2 天前');
	assert.equal(formatPlayMoment(now + 60_000, now), '刚刚');
});

test('launch counts hide a zero value', () => {
	assert.equal(formatPlayCount(0), '—');
	assert.equal(formatPlayCount(Number.NaN), '—');
	assert.equal(formatPlayCount(3), '3 次');
});

test('most played breaks ties by the latest session', () => {
	const value = account({
		instances: {
			first: { totalMs: 60_000, launches: 1, lastPlayedAt: 200 },
			second: { totalMs: 60_000, launches: 1, lastPlayedAt: 300 },
			least: { totalMs: 30_000, launches: 1, lastPlayedAt: 400 },
		},
	});
	assert.equal(mostPlayedInstance(value), 'second');
	assert.equal(mostPlayedInstance(account()), null);
	assert.equal(mostPlayedInstance(null), null);
});

test('account lookup filters by the current account id', () => {
	const value: PlayStatsSnapshot = {
		schemaVersion: 1,
		accounts: {
			one: account({ totalMs: 60_000 }),
			two: account({ totalMs: 30_000 }),
		},
	};
	assert.equal(playStatsAccount(value, 'two')?.totalMs, 30_000);
	assert.equal(playStatsAccount(value, 'missing'), null);
	assert.equal(playStatsAccount(value, null), null);
	assert.equal(playStatsAccount(null, 'one'), null);
});
