import { fileURLToPath } from 'node:url';
import { messages } from '../apps/desktop/src/lib/i18n/index.ts';
import { compareMessages, inspectTree } from './lib/desktop-i18n-guard.mjs';

const issues = [
	...compareMessages(messages['zh-CN'], messages.en),
	...await inspectTree(fileURLToPath(new URL('../apps/desktop/src/', import.meta.url)), messages),
];
if (issues.length) {
	console.error(issues.join('\n'));
	process.exitCode = 1;
} else console.log('Desktop i18n guard passed: dictionary parity and source text.');
