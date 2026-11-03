import { readdir, readFile } from 'node:fs/promises';
import path from 'node:path';

export function compareMessages(source, translated, key = 'messages') {
	const issues = [];
	const kind = value => Array.isArray(value) ? 'array' : value === null ? 'null' : typeof value;
	if (kind(source) !== kind(translated)) return [`${key}: translation type differs`];
	if (typeof source === 'function') {
		if (source.length !== translated.length) issues.push(`${key}: interpolation arity differs`);
	} else if (source && typeof source === 'object') {
		for (const name of Object.keys(source)) {
			if (!Object.hasOwn(translated, name)) issues.push(`${key}.${name}: missing translation`);
			else issues.push(...compareMessages(source[name], translated[name], `${key}.${name}`));
		}
		for (const name of Object.keys(translated)) {
			if (!Object.hasOwn(source, name)) issues.push(`${key}.${name}: unexpected translation`);
		}
		if (Array.isArray(source) && source.length !== translated.length) issues.push(`${key}: array length differs`);
	}
	return issues;
}

export function inspectSource(filename, text, messages) {
	const issues = [];
	let scanned = text;
	if (filename === 'app.html') {
		for (const [locale, dictionary] of Object.entries(messages)) {
			const markup = `<span lang="${locale}" data-startup-lang="${locale}">${dictionary.common.loading}</span>`;
			if (text.split(markup).length !== 2) issues.push(`${filename}: startup copy for ${locale} must occur once and match its dictionary`);
			// Mask only verified dictionary text, preserving line/column positions.
			else scanned = scanned.replace(markup, markup.replace(dictionary.common.loading, dictionary.common.loading.replace(/[^\r\n]/gu, ' ')));
		}
	}
	for (const match of scanned.matchAll(/\p{Script=Han}+/gu)) {
		const prefix = scanned.slice(0, match.index);
		const line = prefix.split('\n').length;
		const column = match.index - prefix.lastIndexOf('\n');
		issues.push(`${filename}:${line}:${column}: Chinese text outside the dictionary`);
	}
	return issues;
}

export async function inspectTree(root, messages) {
	const issues = [];
	async function visit(relative = '') {
		for (const entry of await readdir(path.join(root, relative), { withFileTypes: true })) {
			const filename = path.posix.join(relative, entry.name);
			if (filename === 'lib/i18n') continue;
			if (entry.isDirectory()) await visit(filename);
			else if (entry.isFile() && /\.(?:ts|js|mjs|svelte|html|css|svg|json)$/.test(filename)) {
				issues.push(...inspectSource(filename, await readFile(path.join(root, filename), 'utf8'), messages));
			}
		}
	}
	await visit();
	return issues;
}
