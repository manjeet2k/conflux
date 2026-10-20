// src/components/ui/WindowsDropdown.tsx
import React from 'react';
import { Dropdown, Option } from '@fluentui/react-components';
import { dropdownLayout } from '@/utils/dropdownLayout';
import './WindowsDropdown.css';

export interface WindowsDropdownProps<T extends string> {
  label: string;
  value: T;
  options: Record<T, string>;
  onChange: (value: T) => void;
  width?: number;
  disabled?: boolean;
}

/**
 * WindowsDropdown
 *
 * WinUI ComboBox appearance combined with Fluent UI accessibility and keyboard navigation.
 */
export function WindowsDropdown<T extends string>({
  label,
  value,
  options,
  onChange,
  width = 130,
  disabled = false,
}: WindowsDropdownProps<T>) {
  const values = Object.keys(options) as T[];
  const selectedIndex = Math.max(0, values.indexOf(value));
  const [open, setOpen] = React.useState(false);
  const [anchorIndex, setAnchorIndex] = React.useState(selectedIndex);

  const popupIndex = open ? anchorIndex : selectedIndex;
  const layout = dropdownLayout(window.devicePixelRatio || 1, popupIndex, values.length);

  const revealStyle = {
    '--cfx-dropdown-border': `${layout.border}px`,
    '--cfx-dropdown-padding': `${layout.padding}px`,
    '--cfx-dropdown-margin-y': `${layout.marginY}px`,
    '--cfx-dropdown-margin-x': `${layout.marginX}px`,
    '--cfx-dropdown-row-height': `${layout.rowHeight}px`,
    '--cfx-dropdown-min-width': `${layout.minWidth}px`,
    '--cfx-dropdown-item-padding': `${layout.snap(5)}px ${layout.snap(11)}px ${layout.snap(7)}px`,
    '--cfx-dropdown-reveal-top': `${layout.selectedTop + layout.rowHeight / 2}px`,
    '--cfx-dropdown-reveal-bottom': `${layout.revealBottom + layout.rowHeight / 2}px`,
  } as React.CSSProperties;

  return (
    <Dropdown
      aria-label={label}
      className="cfx-windows-dropdown"
      style={{ width: layout.snap(width), minWidth: layout.snap(width) }}
      disabled={disabled}
      open={open}
      onOpenChange={(_, data) => {
        if (data.open) setAnchorIndex(selectedIndex);
        setOpen(data.open);
      }}
      onOptionSelect={(_, data) => {
        if (data.optionValue) onChange(data.optionValue as T);
      }}
      value={options[value]}
      selectedOptions={[value]}
      listbox={{
        className: 'cfx-windows-dropdown-listbox',
        style: revealStyle,
      }}
    >
      {values.map((k) => (
        <Option key={k} value={k}>
          {options[k]}
        </Option>
      ))}
    </Dropdown>
  );
}
