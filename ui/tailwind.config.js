/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        fluent: {
          bg: '#141414',
          subnav: '#1c1c1c',
          card: '#242424',
          'card-hover': '#2d2d2d',
          'card-selected': '#333333',
          border: 'rgba(255, 255, 255, 0.08)',
          'border-focus': 'rgba(255, 255, 255, 0.2)',
          accent: '#60cdff',
          'accent-hover': '#78d4ff',
          ethernet: '#00f2fe',
          wifi: '#f355da',
          cellular: '#ffb703',
        },
      },
      fontFamily: {
        sans: ['"Segoe UI Variable"', '"Segoe UI"', 'system-ui', 'sans-serif'],
        mono: ['"Cascadia Code"', '"Consolas"', 'monospace'],
      },
      boxShadow: {
        'fluent-card': '0 4px 12px rgba(0, 0, 0, 0.25)',
        'fluent-elevated': '0 12px 32px rgba(0, 0, 0, 0.45)',
      },
    },
  },
  plugins: [],
}
