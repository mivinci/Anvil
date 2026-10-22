import type { AccountLoginOperation } from './runtime';

export type LoginPhase = 'intro' | 'preparing' | 'code' | 'expired' | 'verifying' | 'saving' | 'cancelling' | 'success' | 'failed' | 'cancelled';

export function loginPhase(
	operation: AccountLoginOperation | null,
	starting: boolean,
	cancelling: boolean,
	now: number,
): LoginPhase {
	if (operation?.state === 'completed') return 'success';
	if (operation?.state === 'failed') return 'failed';
	if (operation?.state === 'cancelled') return 'cancelled';
	if (cancelling || operation?.cancelRequested) return 'cancelling';
	if (!operation) return starting ? 'preparing' : 'intro';
	if (operation.stage === 'saving_account') return 'saving';
	if (operation.stage === 'checking_profile') return 'verifying';
	if (operation.authorization) return operation.authorization.expiresAt * 1_000 > now ? 'code' : 'expired';
	return 'preparing';
}

/** Only Microsoft HTTPS destinations are opened automatically during sign-in. */
export function verificationUrl(value: string | undefined): string | null {
	if (!value || value.length > 4_096) return null;
	try {
		const url = new URL(value);
		const microsoftHost = url.hostname === 'microsoft.com' || url.hostname.endsWith('.microsoft.com')
			|| url.hostname === 'login.microsoftonline.com' || url.hostname === 'login.live.com';
		return url.protocol === 'https:' && microsoftHost && !url.username && !url.password && !url.port
			? url.href : null;
	} catch {
		return null;
	}
}

export function remainingTime(expiresAt: number, now: number): string {
	const seconds = Math.max(0, Math.ceil((expiresAt * 1_000 - now) / 1_000));
	return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
}
