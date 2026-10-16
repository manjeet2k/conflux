import React from 'react';
import icon16 from '../assets/app/icon-16.png';
import icon20 from '../assets/app/icon-20.png';
import icon24 from '../assets/app/icon-24.png';
import icon32 from '../assets/app/icon-32.png';
import icon48 from '../assets/app/icon-48.png';
import icon64 from '../assets/app/icon-64.png';
import icon256 from '../assets/app/icon-256.png';

/** Bird/download mark with size-specific artwork for small displays and higher DPI. */
export const AppLogo: React.FC<{ size?: number }> = ({ size = 16 }) => (
  <img
    src={icon16}
    srcSet={`${icon16} 16w, ${icon20} 20w, ${icon24} 24w, ${icon32} 32w, ${icon48} 48w, ${icon64} 64w, ${icon256} 256w`}
    sizes={`${size}px`}
    width={size}
    height={size}
    alt=""
    aria-hidden
    draggable={false}
  />
);
