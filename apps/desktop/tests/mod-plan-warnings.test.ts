import { test } from 'node:test';
import assert from 'node:assert/strict';
import { modPlanWarnings } from '../src/lib/mod-plan-warnings.ts';

test('routine notices are translated and folded by stable code, independent of source wording', () => {
	const values = [{ code: 'fabric_required_only', message: 'upstream wording changed' }, { code: 'read_only_preview', message: 'anything' }];
	const en = modPlanWarnings(['legacy wording'], values, 'en');
	assert.match(en.details[0]!, /Installs compatible Fabric/);
	assert.deepEqual(en.important, []);
	assert.deepEqual(en.technical, []);
	assert.match(modPlanWarnings([], values, 'zh-CN').details[0]!, /安装兼容/);
});
test('unknown and legacy notices stay visible without matching or hiding Chinese prefixes', () => {
	const raw = ['这是只读依赖预览，upstream changed', '需要检查已有配置冲突。'];
	const legacy = modPlanWarnings(raw, undefined, 'en');
	assert.equal(legacy.details.length, 0);
	assert.match(legacy.important[0]!, /unrecognized notice/);
	assert.deepEqual(legacy.technical, raw);
	assert.deepEqual(modPlanWarnings(raw, [], 'en'), legacy);
	const unknown = modPlanWarnings([], [{ code: 'future_warning', message: 'raw warning' }], 'en');
	assert.equal(unknown.important.length, 1);
	assert.deepEqual(unknown.technical, ['raw warning']);
	assert.deepEqual(modPlanWarnings([], undefined, 'en'), { details: [], important: [], technical: [] });
});
