import { beforeEach, expect, test, vi } from 'vitest';
const api = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: api.invoke, isTauri: () => true }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));
beforeEach(() => { vi.resetModules(); api.invoke.mockReset(); });

test('taskbar controls accept their non-sensitive unstamped DTOs and retain response identity checks', async () => {
  const { getTaskbarPreferences, getTaskbarStatus, setTaskbarPreferences, retryTaskbarEmbed } = await import('./runtime');
  api.invoke.mockImplementation(async (_command, args) => ({ api_version: 1, request_id: args.requestId, data: null }));
  await expect(getTaskbarPreferences()).resolves.toBeNull();
  await expect(getTaskbarStatus()).resolves.toBeNull();
  await expect(setTaskbarPreferences({ preferences: {}, expected_settings_revision: '9007199254740993' } as never)).resolves.toBeNull();
  await expect(retryTaskbarEmbed()).resolves.toBeUndefined();
  api.invoke.mockImplementationOnce(async () => ({ api_version: 1, request_id: 'wrong-request', data: null }));
  await expect(getTaskbarStatus()).rejects.toThrow('响应身份不匹配');
});

test('old serialized page after protection is discarded and original continuation is released', async () => {
  const { displayPolicy } = await import('./display-policy');
  const { querySessions } = await import('./runtime');
  displayPolicy.accept({ settings_revision: '1', privacy: false });
  let release!: (value: unknown) => void;
  let id = '';
  api.invoke.mockImplementation((command, args) => {
    if (command === 'query_sessions') { id = args.requestId; return new Promise(resolve => { release = resolve; }); }
    return Promise.resolve(null);
  });
  const original = { query: { marker: 'original-query' }, cursor: null };
  const pending = querySessions(original as never);
  displayPolicy.accept({ settings_revision: '2', privacy: true });
  const cursor = 'a'.repeat(151);
  release({ api_version: 1, request_id: id, display_policy: { settings_revision: '1', privacy: false }, data: { next_cursor: cursor, sessions: [{ display_name: 'SECRET' }] } });
  await expect(pending).rejects.toThrow('丢弃旧响应');
  expect(api.invoke).toHaveBeenLastCalledWith('close_query_snapshot', { requestId: expect.any(String), request: { kind: 'sessions', request: { ...original, cursor } } });
  expect(displayPolicy.get().privacy).toBe(true);
});

test('enable seals before invoke, conflict stays hidden and explicit no-op disable can recover', async () => {
  const { displayPolicy } = await import('./display-policy');
  const { setDisplayPrivacy } = await import('./runtime');
  displayPolicy.accept({ settings_revision: '1', privacy: false });
  api.invoke.mockImplementationOnce(async () => { expect(displayPolicy.get()).toMatchObject({ privacy: true, pending: true }); throw { code: 'REVISION_CONFLICT' }; });
  await expect(setDisplayPrivacy({ privacy: true, expected_settings_revision: '0' })).rejects.toMatchObject({ code: 'REVISION_CONFLICT' });
  expect(displayPolicy.get()).toMatchObject({ privacy: true, pending: false, failure: expect.any(String) });
  api.invoke.mockImplementationOnce(async (_command, args) => ({ api_version: 1, request_id: args.requestId, display_policy: { settings_revision: '1', privacy: false }, data: { preferences: { privacy: false }, settings_revision: '1' } }));
  await setDisplayPrivacy({ privacy: false, expected_settings_revision: '1' });
  expect(displayPolicy.get()).toMatchObject({ privacy: false, failure: null, epoch: 2 });
});

test('sensitive unversioned responses are refused instead of guessing a privacy default', async () => {
  const { getAppStatus } = await import('./runtime');
  api.invoke.mockImplementationOnce(async (_command, args) => ({ api_version: 1, request_id: args.requestId, data: { data_directory: 'SECRET' } }));
  await expect(getAppStatus()).rejects.toThrow('丢弃旧响应');
});

test('late account program selection is discarded and its separate capability released after privacy seals', async () => {
  const { displayPolicy } = await import('./display-policy');
  const { chooseAccountService, cancelAccountServiceSelection } = await import('./runtime');
  displayPolicy.accept({ settings_revision: '1', privacy: false });
  let resolve!: (value: unknown) => void;
  let id = '';
  api.invoke.mockImplementation((command, args) => {
    if (command === 'choose_account_service') { id = args.requestId; return new Promise(done => { resolve = done; }); }
    return Promise.resolve({ api_version: 1, request_id: args.requestId, data: null });
  });
  const pending = chooseAccountService({ kind: 'executable', expected_settings_revision: '1', base_selection_handle: null });
  displayPolicy.enable();
  resolve({ api_version: 1, request_id: id, display_policy: { settings_revision: '1', privacy: false }, data: { selection_handle: 'account-only-capability', preview: { executable_display_path: 'SECRET' } } });
  await expect(pending).rejects.toThrow('丢弃旧响应');
  expect(api.invoke).toHaveBeenLastCalledWith('cancel_account_service_selection', { requestId: expect.any(String), selectionHandle: 'account-only-capability' });
  await expect(cancelAccountServiceSelection('another-account-capability')).resolves.toBeNull();
});
