/**
 * File-type icons — extension → emoji.
 * Phase 8 polish will replace these with SVG icons.
 *
 * Note: directory/file filtering (hidden files, skipped dirs) is done by the
 * Rust backend (`fs_ops.rs`). This file only provides icon mapping.
 */

export function getFileIcon(name: string, isDirectory: boolean): string {
  if (isDirectory) return '📁';
  const ext = name.substring(name.lastIndexOf('.')).toLowerCase();
  switch (ext) {
    case '.tw':
    case '.twee':
      return '📘'; // blue book — Twee passage file
    case '.js':
    case '.mjs':
      return '📜';
    case '.css':
      return '🎨';
    case '.json':
      return '⚙';
    case '.png':
    case '.jpg':
    case '.jpeg':
    case '.gif':
    case '.svg':
    case '.webp':
    case '.bmp':
      return '🖼';
    case '.ogg':
    case '.mp3':
    case '.wav':
    case '.flac':
    case '.m4a':
      return '🎵';
    case '.ttf':
    case '.otf':
    case '.woff':
    case '.woff2':
      return '🔤';
    case '.md':
    case '.txt':
      return '📝';
    case '.html':
    case '.htm':
      return '🌐';
    default:
      return '📄';
  }
}
