/** Existing Runtime data must be confirmed before treating an installation as new. */
export type ExistingWelcomeData = 'present' | 'empty' | 'unknown';
export const welcomeStorageKey = 'anvil.welcome';
const preferenceKeys = ['anvil.locale', 'anvil.appearance', 'anvil.theme', 'anvil.ui-scale', 'anvil.recent-plays'] as const;
export type WelcomeStorage = Pick<Storage, 'getItem' | 'setItem'>;

export class WelcomeState {
	visible = false;
	requiresData = false;
	storageFailed = false;

	private storage: WelcomeStorage;

	constructor(storage: WelcomeStorage, existingData: ExistingWelcomeData) {
		this.storage = storage;
		let marker: string | null;
		let hasPreferences: boolean;
		try {
			marker = storage.getItem(welcomeStorageKey);
			// An interrupted welcome takes priority over preferences selected during it.
			if (marker === 'complete') return;
			if (marker === 'pending') { this.visible = true; return; }
			hasPreferences = preferenceKeys.some(key => storage.getItem(key) !== null);
		} catch {
			this.storageFailed = true;
			// Storage cannot establish whether this is an existing installation.
			return;
		}
		if (hasPreferences || existingData === 'present') { this.persist('complete'); return; }
		if (existingData === 'unknown') { this.requiresData = true; return; }
		this.visible = true;
		this.persist('pending');
	}

	complete() {
		this.persist('complete');
		// Failure to persist must never lock the user out of the application.
		this.visible = false;
	}

	private persist(value: 'pending' | 'complete') {
		try { this.storage.setItem(welcomeStorageKey, value); }
		catch { this.storageFailed = true; }
	}
}
