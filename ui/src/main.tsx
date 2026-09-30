import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import './index.css';
import App from './App.tsx';

async function boot() {
  // Outside Tauri (plain browser during development) run against a simulated backend.
  if (import.meta.env.DEV && !('__TAURI_INTERNALS__' in window)) {
    const { installMockBackend } = await import('./dev/mockBackend');
    installMockBackend();
  }
  createRoot(document.getElementById('root')!).render(
    <StrictMode>
      <App />
    </StrictMode>
  );
}

boot();
