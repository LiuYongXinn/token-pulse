import type { DisplayPolicyStamp } from './generated/contracts';

export type DisplayGate = Readonly<{ privacy: boolean | null; epoch: number; revision: string | null; pending: boolean; failure: string | null }>;
/** Separate from frozen query revisions. A local enable seals retained views before IPC. */
export class DisplayPolicyGate {
  private value: DisplayGate = { privacy: null, epoch: 0, revision: null, pending: false, failure: null };
  private listeners = new Set<() => void>();
  get = () => this.value;
  subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
  private set(value: DisplayGate) { this.value = value; for (const listener of this.listeners) listener(); }
  enable() {
    this.set({ ...this.value, privacy: true, epoch: this.value.epoch + 1, pending: true, failure: null });
  }
  failed(message: string) { this.set({ ...this.value, pending: false, failure: message }); }
  accept(stamp: DisplayPolicyStamp, explicitDisable = false): boolean {
    if (typeof stamp.privacy !== 'boolean' || typeof stamp.settings_revision !== 'string' || !/^(0|[1-9][0-9]*)$/.test(stamp.settings_revision) || stamp.settings_revision.length > 39) throw new Error('显示隐私策略无效，已停止显示响应。');
    const old = this.value;
    if (old.revision !== null && BigInt(stamp.settings_revision) < BigInt(old.revision)) return false;
    // Unconfirmed local protection remains sealed after failure until explicit successful disable.
    if ((old.pending || old.failure !== null) && !stamp.privacy && !explicitDisable) return false;
    if (old.revision === stamp.settings_revision && old.privacy !== null && stamp.privacy !== old.privacy && !explicitDisable && !old.pending && old.failure === null) return false;
    const changed = old.privacy !== null && old.privacy !== stamp.privacy;
    if (old.revision !== stamp.settings_revision || changed || old.privacy === null || old.pending || old.failure !== null) {
      this.set({ privacy: stamp.privacy, epoch: old.epoch + (changed ? 1 : 0), revision: stamp.settings_revision, pending: false, failure: null });
    }
    return true;
  }
}
export const displayPolicy = new DisplayPolicyGate();
