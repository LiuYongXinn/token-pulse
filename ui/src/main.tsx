import React from 'react';
import ReactDOM from 'react-dom/client';
import { App } from './app/App';
import './shared/theme.css';
import { MiniApp } from './mini/MiniApp';

ReactDOM.createRoot(document.getElementById('root')!).render(<React.StrictMode>{new URLSearchParams(location.search).get('window') === 'mini' ? <MiniApp /> : <App />}</React.StrictMode>);
