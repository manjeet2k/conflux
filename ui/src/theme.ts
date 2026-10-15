import { createDarkTheme, createLightTheme, mergeClasses } from '@fluentui/react-components';
import type { BrandVariants, ButtonState, FluentProviderProps, Theme } from '@fluentui/react-components';
import type { Backdrop } from './types';

// Ramp around the Windows 11 default accent (#005FB8 light-mode accent fill at 80).
const windowsBlue: BrandVariants = {
  10: '#00101F',
  20: '#001A33',
  30: '#002447',
  40: '#002E5C',
  50: '#003A73',
  60: '#00468A',
  70: '#0052A1',
  80: '#005FB8',
  90: '#1A70C2',
  100: '#3382CC',
  110: '#4D94D6',
  120: '#66A6DF',
  130: '#80B8E8',
  140: '#99CAF0',
  150: '#B3DCF7',
  160: '#CCEDFF',
};

const fontFamilyBase =
  "'Segoe UI Variable Text', 'Segoe UI Variable', 'Segoe UI', -apple-system, BlinkMacSystemFont, system-ui, sans-serif";
const fontFamilyMonospace = "'Cascadia Mono', Consolas, 'Courier New', monospace";

// Fluent's styling extension also covers buttons rendered in dialog and toast portals.
export const windowsControlStyleHooks: NonNullable<FluentProviderProps['customStyleHooks_unstable']> = {
  useButtonStyles_unstable: (baseState) => {
    const state = baseState as ButtonState;
    state.root.className = mergeClasses(
      state.root.className,
      'cfx-button',
      `cfx-button-${state.appearance}`,
      `cfx-button-${state.size}`,
      state.iconOnly && 'cfx-button-icon-only',
      (state.disabled || state.disabledFocusable) && 'cfx-button-disabled',
    );
  },
};

export const lightTheme: Theme = {
  ...createLightTheme(windowsBlue),
  fontFamilyBase,
  fontFamilyMonospace,
};

// Windows 11 dark mode uses a light accent fill (#60CDFF) with black foreground.
export const darkTheme: Theme = {
  ...createDarkTheme(windowsBlue),
  fontFamilyBase,
  fontFamilyMonospace,
  colorBrandBackground: '#60CDFF',
  colorBrandBackgroundHover: '#5AB9E6',
  colorBrandBackgroundPressed: '#52A6CC',
  colorBrandBackgroundSelected: '#5AB9E6',
  colorCompoundBrandBackground: '#60CDFF',
  colorCompoundBrandBackgroundHover: '#5AB9E6',
  colorCompoundBrandBackgroundPressed: '#52A6CC',
  colorCompoundBrandStroke: '#60CDFF',
  colorCompoundBrandStrokeHover: '#5AB9E6',
  colorCompoundBrandStrokePressed: '#52A6CC',
  colorBrandStroke1: '#60CDFF',
  colorBrandForeground1: '#60CDFF',
  colorBrandForeground2: '#99EBFF',
  colorBrandForegroundLink: '#99EBFF',
  colorBrandForegroundLinkHover: '#B8F2FF',
  colorNeutralForegroundOnBrand: '#000000',
  colorStrokeFocus2: '#FFFFFF',
};

/**
 * Surface colors that Fluent's tokens don't cover: the Win11 layering model of a window
 * backdrop (a material such as Mica, or a solid fallback), a translucent content layer, and
 * cards on it. Exposed as CSS variables on the app root.
 */
export function surfaceVars(dark: boolean, backdrop: Backdrop): Record<string, string> {
  const material = backdrop !== 'none';
  // Acrylic lets more of the desktop through than Mica, so the layer above it is denser.
  const acrylic = backdrop === 'acrylic';
  if (dark) {
    return {
      '--cfx-window': material ? 'transparent' : '#202020',
      '--cfx-layer': acrylic ? 'rgba(40, 40, 40, 0.55)' : material ? 'rgba(58, 58, 58, 0.30)' : '#272727',
      '--cfx-card': 'rgba(255, 255, 255, 0.05)',
      '--cfx-card-hover': 'rgba(255, 255, 255, 0.08)',
      '--cfx-stroke': 'rgba(255, 255, 255, 0.08)',
      '--cfx-divider': 'rgba(255, 255, 255, 0.0837)',
      '--cfx-subtle-hover': 'rgba(255, 255, 255, 0.06)',
      '--cfx-subtle-pressed': 'rgba(255, 255, 255, 0.04)',
      '--cfx-track': 'rgba(255, 255, 255, 0.12)',
      '--cfx-accent': '#60CDFF',
      '--cfx-control-fill': 'rgba(255, 255, 255, 0.0605)',
      '--cfx-control-fill-hover': 'rgba(255, 255, 255, 0.0837)',
      '--cfx-control-fill-pressed': 'rgba(255, 255, 255, 0.0326)',
      '--cfx-control-fill-disabled': 'rgba(255, 255, 255, 0.0419)',
      '--cfx-control-stroke': 'rgba(255, 255, 255, 0.0698)',
      '--cfx-control-bottom-stroke': 'rgba(0, 0, 0, 0.20)',
      '--cfx-flyout-fill': 'rgba(32, 32, 32, 0.96)',
      '--cfx-flyout-stroke': 'rgba(0, 0, 0, 0.20)',
      '--cfx-flyout-shadow': '0 8px 16px rgba(0, 0, 0, 0.26), 0 2px 4px rgba(0, 0, 0, 0.16)',
      '--cfx-header': material ? 'rgba(40, 40, 40, 0.60)' : '#272727',
      '--cfx-selected': 'rgba(255, 255, 255, 0.07)',
      '--cfx-selected-hover': 'rgba(255, 255, 255, 0.10)',
    };
  }
  return {
    '--cfx-window': material ? 'transparent' : '#F3F3F3',
    '--cfx-layer': acrylic ? 'rgba(252, 252, 252, 0.65)' : material ? 'rgba(255, 255, 255, 0.50)' : '#F9F9F9',
    '--cfx-card': 'rgba(255, 255, 255, 0.70)',
    '--cfx-card-hover': 'rgba(249, 249, 249, 0.50)',
    '--cfx-stroke': 'rgba(0, 0, 0, 0.0578)',
    '--cfx-divider': 'rgba(0, 0, 0, 0.0803)',
    '--cfx-subtle-hover': 'rgba(0, 0, 0, 0.0373)',
    '--cfx-subtle-pressed': 'rgba(0, 0, 0, 0.0241)',
    '--cfx-track': 'rgba(0, 0, 0, 0.10)',
    '--cfx-accent': '#005FB8',
    '--cfx-control-fill': 'rgba(255, 255, 255, 0.70)',
    '--cfx-control-fill-hover': 'rgba(249, 249, 249, 0.50)',
    '--cfx-control-fill-pressed': 'rgba(249, 249, 249, 0.30)',
    '--cfx-control-fill-disabled': 'rgba(249, 249, 249, 0.30)',
    '--cfx-control-stroke': 'rgba(0, 0, 0, 0.0578)',
    '--cfx-control-bottom-stroke': 'rgba(0, 0, 0, 0.1622)',
    '--cfx-flyout-fill': 'rgba(252, 252, 252, 0.96)',
    '--cfx-flyout-stroke': 'rgba(0, 0, 0, 0.0578)',
    '--cfx-flyout-shadow': '0 8px 16px rgba(0, 0, 0, 0.14), 0 2px 4px rgba(0, 0, 0, 0.08)',
    '--cfx-header': material ? 'rgba(252, 252, 252, 0.70)' : '#F9F9F9',
    '--cfx-selected': 'rgba(0, 0, 0, 0.0373)',
    '--cfx-selected-hover': 'rgba(0, 0, 0, 0.0578)',
  };
}

/** Distinct series colors for adapters. The accent is left out: it marks the aggregate. */
export const seriesColors = ['#E3008C', '#CA5010', '#107C10', '#8764B8', '#038387', '#986F0B'];
export const seriesColorsDark = ['#FF8AD8', '#FFA26B', '#6CCB5F', '#C5A8F0', '#4FD8DB', '#F2C661'];
