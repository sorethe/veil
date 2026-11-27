export type IconName =
  | "overview"
  | "membrane"
  | "profiles"
  | "routing"
  | "outbounds"
  | "connections"
  | "logs"
  | "diagnostics"
  | "settings"
  | "copy"
  | "check"
  | "close"
  | "chevronDown"
  | "chevronRight"
  | "refresh"
  | "trash"
  | "plus"
  | "power"
  | "search"
  | "folder"
  | "clipboard"
  | "export"
  | "import"
  | "warning"
  | "error"
  | "info"
  | "edit"
  | "link"
  | "qr"
  | "file"
  | "download"
  | "upload"
  | "sun"
  | "moon"
  | "arrowUp"
  | "arrowDown"
  | "external";

const paths: Record<IconName, string> = {
  overview: '<path d="M4 13h6v7H4zM14 4h6v7h-6zM4 4h6v5H4zM14 15h6v5h-6z"/>',
  membrane:
    '<circle cx="12" cy="12" r="7"/><ellipse cx="12" cy="12" rx="3" ry="7"/><path d="M5.4 9.5h13.2M5.4 14.5h13.2"/><ellipse cx="12" cy="12" rx="9.4" ry="3.6" transform="rotate(-28 12 12)"/>',
  profiles: '<path d="M12 3 21 8l-9 5-9-5 9-5Z"/><path d="m3 13 9 5 9-5"/>',
  routing: '<path d="M4 6h5.5a4 4 0 0 1 4 4 4 4 0 0 0 4 4H20"/><path d="M4 18h3M17 6h3"/>',
  outbounds: '<path d="M4 12h16M15 6l6 6-6 6M4 6h5M4 18h5"/>',
  connections:
    '<path d="M9.5 14.5 14.5 9.5"/><path d="M8 16.5 6.5 18a3.54 3.54 0 0 1-5-5l3-3a3.54 3.54 0 0 1 5 0"/><path d="M16 7.5 17.5 6a3.54 3.54 0 0 1 5 5l-3 3a3.54 3.54 0 0 1-5 0"/>',
  logs: '<path d="M4 6h16M4 10h10M4 14h16M4 18h8"/>',
  diagnostics: '<path d="M3 12h4l2-6 4 12 2-6h6"/>',
  settings:
    '<path d="M4 7h16M4 12h16M4 17h16"/><circle cx="9" cy="7" r="2"/><circle cx="15" cy="12" r="2"/><circle cx="8" cy="17" r="2"/>',
  copy: '<rect x="8" y="8" width="12" height="12"/><path d="M16 8V4H4v12h4"/>',
  check: '<path d="m4 12 5 5L20 6"/>',
  close: '<path d="M5 5l14 14M19 5 5 19"/>',
  chevronDown: '<path d="m6 9 6 6 6-6"/>',
  chevronRight: '<path d="m9 6 6 6-6 6"/>',
  refresh: '<path d="M20 12a8 8 0 1 1-2.34-5.66M20 4v5h-5"/>',
  trash: '<path d="M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13M10 11v5M14 11v5"/>',
  plus: '<path d="M12 4v16M4 12h16"/>',
  power: '<path d="M12 3v9"/><path d="M7.05 6.05a7 7 0 1 0 9.9 0"/>',
  search: '<circle cx="10.5" cy="10.5" r="6.5"/><path d="m15.5 15.5 5 5"/>',
  folder: '<path d="M3 6h6l2 3h10v10H3z"/>',
  clipboard: '<path d="M9 4H5v17h14V4h-4"/><rect x="9" y="2" width="6" height="4"/>',
  export: '<path d="M12 16V3m-5 5 5-5 5 5M4 21h16"/>',
  import: '<path d="M12 3v13m-5-5 5 5 5-5M4 21h16"/>',
  warning: '<path d="M12 3 22 20H2L12 3Z"/><path d="M12 9v5M12 17v.01"/>',
  error: '<circle cx="12" cy="12" r="9"/><path d="M12 7v6M12 17v.01"/>',
  info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 7v.01"/>',
  edit: '<path d="M4 20h4L20 8l-4-4L4 16v4Z"/><path d="m14 6 4 4"/>',
  link: '<path d="M10 14a4 4 0 0 0 5.66 0l3-3A4 4 0 0 0 13 5.34l-1.5 1.5"/><path d="M14 10a4 4 0 0 0-5.66 0l-3 3A4 4 0 0 0 11 18.66l1.5-1.5"/>',
  qr: '<path d="M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h2v2h-2zM18 14h2v2h-2zM16 18h4v2h-4zM14 20h1"/>',
  file: '<path d="M6 3h8l4 4v14H6z"/><path d="M14 3v5h4"/>',
  download: '<path d="M12 4v11m-5-5 5 5 5-5M5 20h14"/>',
  upload: '<path d="M12 15V4m-5 5 5-5 5 5M5 20h14"/>',
  sun: '<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/>',
  moon: '<path d="M20 14.5A8.5 8.5 0 0 1 9.5 4 8.5 8.5 0 1 0 20 14.5Z"/>',
  arrowUp: '<path d="M12 19V5m-6 6 6-6 6 6"/>',
  arrowDown: '<path d="M12 5v14m-6-6 6 6 6-6"/>',
  external: '<path d="M14 4h6v6M20 4l-9 9M18 13v7H4V6h7"/>',
};

export function iconMarkup(name: IconName): string {
  return paths[name];
}
