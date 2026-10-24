import { test } from 'node:test';
import assert from 'node:assert/strict';
import { formatModDownloads, formatModUpdated } from '../src/lib/mod-project-stats.ts';

test('download counts compact large values while keeping zero distinct from missing data', () => {
	assert.equal(formatModDownloads(0), '0');
	assert.equal(formatModDownloads(12500), '1.3万');
	assert.equal(formatModDownloads(250000000), '2.5亿');
	for (const value of [null, undefined, -1, 0.5, NaN, Infinity, Number.MAX_SAFE_INTEGER + 1]) assert.equal(formatModDownloads(value), null);
});
test('update dates use the upstream timestamp and a consistent UTC calendar date', () => {
	assert.equal(formatModUpdated('2026-10-24T12:30:00Z'), '2026-10-24');
	assert.equal(formatModUpdated('2026-10-24T00:30:00+08:00'), '2026-10-23');
	for (const value of [null, undefined, '', 'not a date', '2026', '2026-10-24', '2026-99-99T00:00:00Z']) assert.equal(formatModUpdated(value), null);
});
