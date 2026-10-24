/** Shared by sidebar links and source selection buttons. */
export function pageSidebarItemClass(selected: boolean) {
	return `flex h-8 w-full items-center gap-2 rounded-lg px-2.5 text-left text-[13px] transition-colors focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40 ${selected ? 'bg-chrome-selected font-medium text-accent-foreground' : 'text-muted-foreground hover:bg-chrome-hover hover:text-foreground'}`;
}
