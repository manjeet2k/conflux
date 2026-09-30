import React from 'react';
import { fileIcon } from '../utils/files';

/** File-type icon picked from the extension. */
export const FileIcon: React.FC<{ filename: string; className?: string }> = ({ filename, className }) =>
  React.createElement(fileIcon(filename), { className });
