import { api } from '../api';

// A promise is shared while it is pending, so a list of the same type makes one IPC call.
const icons = new Map<string, Promise<string | null>>();
const MAX_CACHED_TYPES = 128;

export function fileExtension(filename: string): string | null {
  const leaf = filename.trim().split(/[\\/]/).pop() ?? '';
  const dot = leaf.lastIndexOf('.');
  const extension = dot > 0 ? leaf.slice(dot + 1).toLowerCase() : '';
  return /^[a-z0-9]{1,32}$/.test(extension) ? extension : null;
}

export function shellIconUrl(extension: string | null, folder: boolean): Promise<string | null> {
  const key = folder ? 'folder' : 'file:' + (extension ?? '');
  const cached = icons.get(key);
  if (cached) return cached;
  const pending = api.getShellIcon(extension, folder).then(icon => {
    if (!icon || icon.width < 1 || icon.width > 256 || icon.height < 1 || icon.height > 256
      || icon.rgba.length !== icon.width * icon.height * 4) return null;
    const canvas = document.createElement('canvas');
    canvas.width = icon.width;
    canvas.height = icon.height;
    const context = canvas.getContext('2d');
    if (!context) return null;
    context.putImageData(new ImageData(Uint8ClampedArray.from(icon.rgba), icon.width, icon.height), 0, 0);
    return canvas.toDataURL('image/png');
  }).catch(error => {
    console.warn('Windows file icon unavailable:', error);
    return null;
  });
  if (icons.size >= MAX_CACHED_TYPES) icons.delete(icons.keys().next().value!);
  icons.set(key, pending);
  return pending;
}
