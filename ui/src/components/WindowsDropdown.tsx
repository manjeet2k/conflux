import React from 'react';
import { Dropdown, Option } from '@fluentui/react-components';
import { dropdownLayout } from '../utils/dropdownLayout';
import './WindowsDropdown.css';

interface WindowsDropdownProps<T extends string> {
  label: string;
  value: T;
  options: Record<T, string>;
  onChange: (value: T) => void;
  width?: number;
  disabled?: boolean;
}

/** WinUI ComboBox visuals with Fluent's selection, keyboard and focus behavior. */
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
        // The recording dismisses within a single frame; there is no closing shrink.
        setOpen(data.open);
      }}
      value={options[value]}
      selectedOptions={[value]}
      listbox={{
        className: 'cfx-windows-dropdown-listbox',
        style: revealStyle,
      }}
      positioning={{
        position: 'below',
        align: 'start',
        coverTarget: true,
        // Native popup width follows its contents rather than the closed button.
        matchTargetSize: undefined,
        // This popup covers the choice rather than flipping to the other side.
        pinned: true,
        overflowBoundaryPadding: 8,
        offset: ({ targetRect, positionedRect }) => {
          // Clamp the entire surface; Fluent's tether otherwise permits a clipped
          // top edge when the selected option sits near the end of the list.
          const top = Math.max(8, Math.min(targetRect.y - layout.selectedTop, window.innerHeight - positionedRect.height - 8));
          return {
            mainAxis: top - targetRect.y,
            // In the recording all popups start at the trailing 130-DIP control area.
            crossAxis: targetRect.width - layout.minWidth,
          };
        },
      }}
      onOptionSelect={(_, data) => {
        if (data.optionValue !== undefined && Object.hasOwn(options, data.optionValue)) {
          onChange(data.optionValue as T);
        }
      }}
    >
      {values.map((option) => (
        <Option key={option} value={option} checkIcon={null}>
          {options[option]}
        </Option>
      ))}
    </Dropdown>
  );
}
