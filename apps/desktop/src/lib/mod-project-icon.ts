import type { ModProjectSummary, ModSource } from './runtime';

export function modProjectIcon(project: ModProjectSummary, source: ModSource): string | null {
	const candidates = source === 'modrinth' ? [project.iconUrl] : [project.logo?.thumbnailUrl, project.logo?.url];
	const hosts = source === 'modrinth' ? ['cdn.modrinth.com'] : ['media.forgecdn.net', 'edge.forgecdn.net'];
	for (const value of candidates) {
		if (!value || value.length > 2048) continue;
		try {
			const url = new URL(value);
			if (url.protocol === 'https:' && hosts.includes(url.hostname) && !url.username && !url.password && !url.port) return url.href;
		} catch { /* Missing or malformed optional artwork does not hide a project. */ }
	}
	return null;
}
