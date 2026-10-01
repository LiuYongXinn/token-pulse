import { expect, test } from 'vitest';
import { DisplayPolicyGate } from './display-policy';

test('latest exact policy wins, a transition changes epoch and old publications cannot unhide', () => {
  const gate = new DisplayPolicyGate();
  expect(gate.get().privacy).toBeNull();
  expect(gate.accept({ settings_revision: '9007199254740993', privacy: false })).toBe(true);
  expect(gate.get().epoch).toBe(0);
  gate.accept({ settings_revision: '9007199254740994', privacy: true });
  expect(gate.get()).toMatchObject({ privacy: true, epoch: 1 });
  expect(gate.accept({ settings_revision: '9007199254740993', privacy: false })).toBe(false);
  expect(gate.accept({ settings_revision: '9007199254740994', privacy: false })).toBe(false);
  expect(gate.get().privacy).toBe(true);
  gate.accept({ settings_revision: '9007199254740995', privacy: false });
  expect(gate.get()).toMatchObject({ privacy: false, epoch: 2 });
});

test('local enable seals before commit and failed persistence needs explicit successful disable', () => {
  const gate = new DisplayPolicyGate();
  gate.accept({ settings_revision: '1', privacy: false });
  gate.enable();
  expect(gate.get()).toMatchObject({ privacy: true, pending: true, epoch: 1 });
  expect(gate.accept({ settings_revision: '1', privacy: false })).toBe(false);
  gate.failed('conflict');
  expect(gate.accept({ settings_revision: '2', privacy: false })).toBe(false);
  expect(gate.get()).toMatchObject({ privacy: true, pending: false, failure: 'conflict' });
  gate.accept({ settings_revision: '2', privacy: false }, true);
  expect(gate.get()).toMatchObject({ privacy: false, pending: false, failure: null, epoch: 2 });
});

test('successful enable acknowledges protection without a second transition; subscriptions clean up', () => {
  const gate = new DisplayPolicyGate();
  gate.accept({ settings_revision: '1', privacy: false });
  let calls = 0; const unsubscribe = gate.subscribe(() => ++calls);
  gate.enable(); gate.accept({ settings_revision: '2', privacy: true });
  expect(gate.get()).toMatchObject({ epoch: 1, pending: false, revision: '2' });
  expect(calls).toBe(2); unsubscribe();
  gate.accept({ settings_revision: '3', privacy: false });
  expect(calls).toBe(2);
});

test('invalid policy never changes the gate or permits a field default', () => {
  const gate = new DisplayPolicyGate();
  gate.accept({ settings_revision: '1', privacy: true });
  const saved = gate.get();
  for (const revision of ['', '-1', '01', '1.1', '9'.repeat(40)]) expect(() => gate.accept({ settings_revision: revision, privacy: false })).toThrow('无效');
  expect(gate.get()).toBe(saved);
});
