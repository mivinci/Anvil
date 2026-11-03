// Runs before app hydration so saved text size applies on the first paint.
(() => {
	let saved = null;
	try { saved = localStorage.getItem('anvil.ui-scale'); }
	catch { document.documentElement.dataset.uiScaleStorageIssue = 'read'; }
	document.documentElement.dataset.uiScale =
		saved === 'large' || saved === 'larger' ? saved : 'default';
})();
