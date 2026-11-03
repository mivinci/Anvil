import { defaultLocale, isLocale, localeStorageKey, messages, resolveLocale, type Locale } from './i18n/index.ts';

export type LocalePreference = Locale | 'system';
export type LocaleStorageIssue = 'read' | 'write' | null;
export interface LocaleEnvironment {
	storage: Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>;
	languages: readonly string[];
	language: string;
	setDocumentLanguage: (locale: Locale) => void;
}

export class LocaleState {
	current = $state<Locale>(defaultLocale);
	preference = $state<LocalePreference>('system');
	storageIssue = $state<LocaleStorageIssue>(null);
	get messages() { return messages[this.current]; }
	get storageMessage(): string | null {
		return this.storageIssue === 'read' ? this.messages.settings.storageReadFailed
			: this.storageIssue === 'write' ? this.messages.settings.sessionOnly : null;
	}

	constructor(private environment?: LocaleEnvironment) {
		if (!environment && typeof window !== 'undefined') {
			// Access localStorage inside each method: even the property getter can throw.
			this.environment = {
				storage: {
					getItem: key => window.localStorage.getItem(key),
					setItem: (key, value) => window.localStorage.setItem(key, value),
					removeItem: key => window.localStorage.removeItem(key),
				},
				languages: navigator.languages,
				language: navigator.language,
				setDocumentLanguage: locale => { document.documentElement.lang = locale; },
			};
		}
		if (!this.environment) return;
		let saved: unknown;
		try { saved = this.environment.storage.getItem(localeStorageKey); }
		catch { this.storageIssue = 'read'; }
		this.preference = isLocale(saved) ? saved : 'system';
		this.apply(resolveLocale(saved, this.environment.languages, this.environment.language));
	}

	setLocale(preference: LocalePreference): void {
		if (preference !== 'system' && !isLocale(preference)) throw new TypeError('Unsupported locale');
		this.preference = preference;
		this.apply(resolveLocale(preference, this.environment?.languages, this.environment?.language));
		if (!this.environment) { this.storageIssue = 'write'; return; }
		try {
			if (preference === 'system') this.environment.storage.removeItem(localeStorageKey);
			else this.environment.storage.setItem(localeStorageKey, preference);
			this.storageIssue = null;
		} catch { this.storageIssue = 'write'; }
	}

	private apply(locale: Locale): void {
		this.current = locale;
		this.environment?.setDocumentLanguage(locale);
	}
}

export const localeState = new LocaleState();
