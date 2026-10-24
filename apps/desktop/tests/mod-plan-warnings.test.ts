import { test } from 'node:test';
import assert from 'node:assert/strict';
import { modPlanWarnings } from '../src/lib/mod-plan-warnings.ts';

test('known routine explanations are folded into installation details', () => {
	const message = '仅安装兼容的 Fabric 主 JAR 和必需依赖；不自动升级或替换已有项目。';
	assert.deepEqual(modPlanWarnings([message]), { details: [message], important: [] });
});
test('unknown warnings remain visible and obsolete read-only wording is omitted', () => {
	assert.deepEqual(modPlanWarnings(['需要检查已有配置冲突。', '这是只读依赖预览，尚未下载或修改实例。']), { details: [], important: ['需要检查已有配置冲突。'] });
});
