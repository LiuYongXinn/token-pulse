const timings = [];
for (let n = 0; n < 10; ++n) { const started = performance.now(); const reply = await window.__TAURI_INTERNALS__.invoke('get_mini_usage', { requestId: crypto.randomUUID() }); if (!reply.data) throw new Error('MINI_READ_FAILED'); timings.push(performance.now()-started); }
await window.__TAURI_INTERNALS__.invoke('plugin:event|emit', { event: MEASUREMENT_EVENT, payload: { milliseconds: timings } });
