import { useState, type RefObject } from 'react';
import type { RegisterAgent as RegisterInput } from '../../ipc/generated/RegisterAgent';
export function RegisterAgent({
  busy,
  onRegister,
  labelInput,
}: {
  busy: boolean;
  labelInput: RefObject<HTMLInputElement | null>;
  onRegister: (input: RegisterInput) => Promise<void>;
}) {
  const [label, setLabel] = useState('');
  const [kind, setKind] = useState<'ClaudeCode' | 'Codex'>('ClaudeCode');
  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        void onRegister({ kind, label }).then(() => {
          setLabel('');
          requestAnimationFrame(() => {
            labelInput.current?.focus();
          });
        });
      }}
    >
      <fieldset disabled={busy}>
        <legend>Register local agent</legend>
        <label htmlFor="agent-kind">Agent kind</label>
        <select
          id="agent-kind"
          value={kind}
          onChange={(e) => {
            setKind(e.target.value === 'Codex' ? 'Codex' : 'ClaudeCode');
          }}
        >
          <option value="ClaudeCode">Claude Code</option>
          <option value="Codex">Codex</option>
        </select>
        <label htmlFor="agent-label">Agent label</label>
        <input
          id="agent-label"
          ref={labelInput}
          required
          maxLength={64}
          value={label}
          onChange={(e) => {
            setLabel(e.target.value);
          }}
        />
        <button type="submit">{busy ? 'Saving…' : 'Register agent'}</button>
      </fieldset>
    </form>
  );
}
