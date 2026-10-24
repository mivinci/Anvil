import type { ModSource } from './runtime';

export interface ModDiscoveryContext {
	hasInstance: boolean;
	instanceName: string | null;
	instanceIssue: 'invalid' | 'duplicate' | null;
	source: ModSource | null;
	sourceIssue: 'invalid' | 'duplicate' | null;
}
type ReadableUrl = { href: string; searchParams: Pick<URLSearchParams, 'getAll'> };

function validName(name: string) {
	return /^[a-zA-Z0-9][a-zA-Z0-9._-]{0,47}$/.test(name) && !name.endsWith('.');
}

/** The URL names a requested target; the Runtime still has to confirm it exists. */
export function readModDiscoveryContext(url: ReadableUrl): ModDiscoveryContext {
	const instances = url.searchParams.getAll('instance');
	const sources = url.searchParams.getAll('source');
	const instance = instances[0] ?? '';
	const source = sources[0] ?? '';
	const instanceIssue = instances.length > 1 ? 'duplicate' : instances.length === 1 && !validName(instance) ? 'invalid' : null;
	const sourceIssue = sources.length > 1 ? 'duplicate' : sources.length === 1 && !['modrinth', 'curseforge'].includes(source) ? 'invalid' : null;
	return {
		hasInstance: instances.length > 0,
		instanceName: instances.length === 1 && !instanceIssue ? instance.toLowerCase() : null,
		instanceIssue,
		source: sourceIssue ? null : sources.length === 0 || source === 'modrinth' ? 'modrinth' : 'curseforge',
		sourceIssue,
	};
}

/** Preserve the search while invalidating a project chosen under another context. */
export function updateModDiscoveryContext(url: ReadableUrl, change: { instance?: string | null; source?: ModSource }): URL {
	const current = readModDiscoveryContext(url);
	const next = new URL(url.href);
	let changed = false;
	if (change.instance !== undefined) {
		if (change.instance !== null && !validName(change.instance)) throw new Error('实例名称无效。');
		const name = change.instance?.toLowerCase() ?? null;
		changed = current.instanceIssue !== null || name !== current.instanceName;
		next.searchParams.delete('instance');
		if (name) next.searchParams.set('instance', name);
	}
	if (change.source !== undefined) {
		if (change.source !== 'modrinth' && change.source !== 'curseforge') throw new Error('模组来源无效。');
		changed ||= current.sourceIssue !== null || change.source !== current.source;
		next.searchParams.delete('source');
		if (change.source !== 'modrinth') next.searchParams.set('source', change.source);
	}
	if (changed) {
		next.searchParams.delete('project');
		next.searchParams.delete('index');
		return next;
	}
	return new URL(url.href);
}
