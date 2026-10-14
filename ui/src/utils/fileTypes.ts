/** Visual categories for downloaded filenames; no filesystem or platform lookup is needed. */
export type FileKind = 'generic' | 'archive' | 'application' | 'disk' | 'image' | 'video' | 'audio'
  | 'pdf' | 'document' | 'spreadsheet' | 'presentation' | 'text' | 'code' | 'font' | 'ebook' | 'database';

export const fileTypeLabels: Record<FileKind, string> = {
  generic: 'File', archive: 'Archive', application: 'Application', disk: 'Disk image',
  image: 'Image', video: 'Video', audio: 'Audio', pdf: 'PDF document', document: 'Document',
  spreadsheet: 'Spreadsheet', presentation: 'Presentation', text: 'Text document',
  code: 'Code / configuration', font: 'Font', ebook: 'E-book', database: 'Database',
};

const groups: [FileKind, string[]][] = [
  ['archive', ['zip', 'rar', '7z', 'gz', 'tgz', 'xz', 'bz2', 'tbz2', 'txz', 'tar', 'zst', 'cab', 'lz', 'lzma']],
  ['application', ['exe', 'msi', 'msix', 'msixbundle', 'appx', 'appxbundle', 'apk', 'deb', 'rpm', 'appimage']],
  ['disk', ['iso', 'img', 'vhd', 'vhdx', 'vmdk', 'dmg', 'vdi', 'nrg']],
  ['video', ['mp4', 'mkv', 'avi', 'mov', 'webm', 'wmv', 'flv', 'm4v', 'mpeg', 'mpg', 'mts', 'm2ts']],
  ['audio', ['mp3', 'flac', 'wav', 'aac', 'ogg', 'm4a', 'opus', 'wma', 'aiff', 'mid', 'midi']],
  ['image', ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'svg', 'tif', 'tiff', 'heic', 'heif', 'avif', 'ico', 'raw', 'psd']],
  ['pdf', ['pdf']],
  ['document', ['doc', 'docx', 'rtf', 'odt', 'pages']],
  ['spreadsheet', ['xls', 'xlsx', 'xlsm', 'xlsb', 'csv', 'tsv', 'ods', 'numbers']],
  ['presentation', ['ppt', 'pptx', 'pps', 'ppsx', 'odp', 'key']],
  ['text', ['txt', 'md', 'markdown', 'log', 'nfo']],
  ['code', ['json', 'xml', 'yaml', 'yml', 'toml', 'ini', 'cfg', 'conf', 'js', 'jsx', 'ts', 'tsx', 'html', 'htm', 'css', 'scss', 'py', 'rs', 'c', 'cpp', 'h', 'cs', 'java', 'go', 'sh', 'ps1', 'bat', 'cmd', 'sql']],
  ['font', ['ttf', 'otf', 'woff', 'woff2', 'eot']],
  ['ebook', ['epub', 'mobi', 'azw', 'azw3', 'fb2', 'cbz', 'cbr']],
  ['database', ['db', 'sqlite', 'sqlite3', 'mdb', 'accdb', 'db3']],
];

export function fileKind(filename: string): FileKind {
  const leaf = filename.trim().split(/[\\/]/).pop()!.toLowerCase();
  if (['.env', '.gitignore', '.editorconfig'].includes(leaf)) return 'code';
  if (/\.(?:zip|7z|rar)\.\d{3}$/.test(leaf)) return 'archive';
  const dot = leaf.lastIndexOf('.');
  if (dot <= 0) return 'generic';
  const extension = leaf.slice(dot + 1);
  return groups.find(([, extensions]) => extensions.includes(extension))?.[0] ?? 'generic';
}
