import {
  Document24Regular,
  DocumentPdf24Regular,
  FolderZip24Regular,
  Image24Regular,
  MusicNote224Regular,
  Video24Regular,
  Code24Regular,
  AppGeneric24Regular,
  DocumentText24Regular,
} from '@fluentui/react-icons';
import type { FluentIcon } from '@fluentui/react-icons';

const groups: [FluentIcon, string[]][] = [
  [FolderZip24Regular, ['zip', 'rar', '7z', 'gz', 'tgz', 'xz', 'bz2', 'tar', 'zst', 'iso', 'img', 'dmg']],
  [AppGeneric24Regular, ['exe', 'msi', 'msix', 'appx', 'apk', 'deb', 'rpm', 'appimage']],
  [Video24Regular, ['mp4', 'mkv', 'avi', 'mov', 'webm', 'wmv', 'flv', 'm4v']],
  [MusicNote224Regular, ['mp3', 'flac', 'wav', 'aac', 'ogg', 'm4a', 'opus']],
  [Image24Regular, ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'svg', 'tiff', 'heic']],
  [DocumentPdf24Regular, ['pdf']],
  [DocumentText24Regular, ['txt', 'md', 'doc', 'docx', 'rtf', 'odt', 'csv', 'xlsx', 'pptx']],
  [Code24Regular, ['json', 'xml', 'js', 'ts', 'py', 'rs', 'c', 'cpp', 'h', 'sh', 'ps1']],
];

export function fileIcon(filename: string): FluentIcon {
  const ext = filename.includes('.') ? filename.split('.').pop()!.toLowerCase() : '';
  for (const [icon, exts] of groups) {
    if (exts.includes(ext)) return icon;
  }
  return Document24Regular;
}

/** Copies text, falling back to execCommand where the async clipboard API is unavailable. */
export async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    return;
  } catch {
    const el = document.createElement('textarea');
    el.value = text;
    el.style.position = 'fixed';
    el.style.opacity = '0';
    document.body.appendChild(el);
    el.select();
    document.execCommand('copy');
    el.remove();
  }
}

/** First http(s) URL in a blob of text, if any. */
export function extractUrl(text: string): string | null {
  const match = text.match(/https?:\/\/[^\s"'<>]+/i);
  return match ? match[0] : null;
}
