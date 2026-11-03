import assert from 'node:assert/strict';
import { test } from 'node:test';
import { cn } from '../src/lib/utils.ts';

test('custom type tokens survive color-class merging', () => {
	const merged = cn('text-body', 'text-muted-foreground');
	assert.match(merged, /text-body/);
	assert.match(merged, /text-muted-foreground/);
	assert.doesNotMatch(merged, /text-caption/);
});

test('later custom type tokens replace earlier ones', () => {
	assert.match(cn('text-body', 'text-caption'), /text-caption/);
	assert.doesNotMatch(cn('text-body', 'text-caption'), /text-body/);
	assert.match(cn('text-xs', 'text-title'), /text-title/);
	assert.doesNotMatch(cn('text-xs', 'text-title'), /text-xs/);
});
