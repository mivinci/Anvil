import assert from 'node:assert/strict';
import { test } from 'node:test';
import { runtimeErrorCode, runtimeErrorMessage, requestErrorMessage } from '../src/lib/runtime-errors.ts';

test('known codes translate independently of detail and unsupported codes use a generic message', () => {
	const error = { code: 'plan_expired', message: 'arbitrary source wording' };
	assert.equal(requestErrorMessage(error, 'en', 'fallback'), 'The installation plan expired. Generate a new plan.');
	assert.equal(requestErrorMessage(error, 'zh-CN', 'fallback'), '安装计划已过期，请重新生成。');
	assert.equal(requestErrorMessage({ code: 'future_error', message: 'raw 中文' }, 'en', 'fallback'), 'The operation failed. Please try again.');
	assert.equal(requestErrorMessage({ message: 'raw 中文' }, 'en', 'context fallback'), 'context fallback');
	assert.equal(error.message, 'arbitrary source wording');
	assert.equal(runtimeErrorMessage('__proto__', 'en'), 'The operation failed. Please try again.');
	assert.equal(runtimeErrorMessage('constructor', 'en'), 'The operation failed. Please try again.');
});
test('malformed code fields never replace contextual fallback or expose their contents', () => {
	for (const code of [null, 1, {}, 'invalid code', 'a'.repeat(65)]) {
		assert.equal(runtimeErrorCode({ code }), null);
		assert.equal(requestErrorMessage({ code }, 'en', 'fallback'), 'fallback');
	}
});
