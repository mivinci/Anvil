// Runs before app hydration under script-src 'self'. Keep matching policy covered by i18n tests.
(() => {
	let saved;
	try { saved = localStorage.getItem('anvil.locale'); }
	catch { document.documentElement.dataset.localeStorageIssue = 'read'; }
	let locale = saved === 'zh-CN' || saved === 'en' ? saved : null;
	if (!locale) {
		for (const language of navigator.languages.length ? navigator.languages : [navigator.language]) {
			const primary = language.toLowerCase().split('-')[0];
			if (primary === 'en') { locale = 'en'; break; }
			if (primary === 'zh') { locale = 'zh-CN'; break; }
		}
	}
	document.documentElement.lang = locale || 'zh-CN';
})();
